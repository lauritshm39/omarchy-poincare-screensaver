//! The screensaver must not crash when its terminal disappears.
//!
//! This is the regression test for the SIGABRT that showed up in
//! `coredumpctl` after every lock: the idle timer draws the screensaver, the
//! lock screen then destroys the terminal window, and every write after that
//! fails with EIO. Reporting that failure with `eprintln!` -- on the same dead
//! pty -- panicked, and `panic = "abort"` turned the panic into a SIGABRT and
//! a core dump.
//!
//! The pty here is deliberately *not* made a controlling terminal. With one,
//! closing the master delivers SIGHUP and the process dies before it can ever
//! reach a failing write, which hides the bug this test is about.

use std::process::{Command, Stdio};
use std::os::unix::io::FromRawFd;
use std::os::unix::process::ExitStatusExt;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn open_pty() -> (i32, i32) {
    let (mut master, mut slave) = (0, 0);
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(rc, 0, "openpty failed");

    // openpty leaves both ends inheritable. The screensaver would then inherit
    // the *master*, and a pty slave only reports EIO once every master fd is
    // closed -- so the child would sit there holding its own terminal open and
    // never notice it had been destroyed. dup(2) and dup2(2) both clear
    // FD_CLOEXEC, so the copies handed to the child as stdio are unaffected.
    for fd in [master, slave] {
        assert_ne!(
            unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) },
            -1,
            "could not set FD_CLOEXEC"
        );
    }
    (master, slave)
}

#[test]
fn destroying_the_terminal_mid_draw_is_a_quiet_exit() {
    let (master, slave) = open_pty();

    let stdio = || unsafe { Stdio::from_raw_fd(libc::dup(slave)) };
    let mut child = Command::new(env!("CARGO_BIN_EXE_omarchy-poincare"))
        .args(["--managed", "--text", "HI"])
        .stdin(stdio())
        .stdout(stdio())
        .stderr(stdio())
        .spawn()
        .expect("failed to spawn the screensaver");
    unsafe { libc::close(slave) };

    // Wait until it has actually written to the terminal, so we know it got as
    // far as the render loop and the teardown below is testing something.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let n = unsafe { libc::read(master, buf.as_mut_ptr().cast(), buf.len()) };
        let _ = tx.send(n);
    });
    let drawn = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("the screensaver never drew anything");
    assert!(drawn > 0, "expected terminal setup on stdout, read {drawn}");

    assert!(
        child.try_wait().unwrap().is_none(),
        "the screensaver exited before its terminal was destroyed, so this \
         test proves nothing"
    );

    unsafe { libc::close(master) }; // the lock screen takes the window away

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the screensaver never noticed its terminal was gone");
        }
        std::thread::sleep(Duration::from_millis(20));
    };

    assert_eq!(
        status.signal(),
        None,
        "the screensaver died from a signal ({:?}); signal 6 is the SIGABRT \
         this test exists to catch",
        status.signal()
    );
    assert!(
        status.success(),
        "losing the terminal is a normal shutdown, expected exit 0, got {status:?}"
    );
}

#[test]
fn a_real_error_is_still_reported() {
    // The risk in treating a dead terminal as a clean exit is swallowing
    // genuine failures along with it.
    let out = Command::new(env!("CARGO_BIN_EXE_omarchy-poincare"))
        .args(["--orbit", "definitely-not-an-orbit"])
        .output()
        .expect("failed to spawn the screensaver");

    assert!(!out.status.success(), "a bad --orbit must not exit 0");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown orbit"),
        "expected the error on stderr, got {stderr:?}"
    );
}
