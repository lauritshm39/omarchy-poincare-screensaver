//! omarchy-screensaver-starry -- a starry-night screensaver for Omarchy.
//!
//! A twinkling starfield drifts across the terminal, then gathers into
//! whatever `omarchy branding screensaver` has set, holds, scatters, and opens
//! a fresh sky. The morph / hold / scatter phases are the shared stage
//! machinery in `omarchy-screensaver-core`.

mod starry;

use omarchy_screensaver_core::config::config_args;
use omarchy_screensaver_core::palette::{self, Palette};
use omarchy_screensaver_core::rng::Rng;
use omarchy_screensaver_core::stage::{self, StageArt, StageOpts};
use omarchy_screensaver_core::{say, sayln, terminal_is_gone, warn};
    use starry::{StarCfg, Starfield};

const HELP: &str = "\
omarchy-screensaver-starry -- starry-night screensaver for Omarchy

USAGE:
    omarchy-screensaver-starry [OPTIONS]

SKY
        --stars <n>            How many stars (default: scaled to the canvas,
                               400..4000)
        --star-size <x>        Star disc multiplier (default: 1.2, 0 keeps all
                               stars as single dots)
        --speed <x>            Outward drift multiplier (default: 2.0)
        --warp <x>             Hyperdrive gain: extra outward speed with
                               distance from centre (default: 1.0, 0 is calm)
        --palette              Show the colours resolved from the current theme

TARGET
    -f, --file <path>          Art to gather into (default: Omarchy branding,
                               ~/.config/omarchy/branding/screensaver.txt)
    -t, --text <string>        Draw this text with the built-in block font
                               instead of reading a file
        --text-scale <n>       Enlarge --text by this factor (default: 1)
        --art-scale <auto|n>   Enlarge block art to suit the terminal size
                               (default: auto)
        --black-hole           Reveal the art over a black hole with a glowing
                               accretion ring (default: on)
        --no-black-hole        Gather into the art with no black hole
        --hole-size <x>        Black-hole radius as a fraction of the smaller
                               canvas dimension (default: 0.3)

APPEARANCE
        --font-size <n>        Terminal font size, and so the resolution of the
                               picture. Applied by the launcher, not here; a
                               smaller size means finer curves (default: 18)
        --stroke <n>           Trail thickness, radius in dots (default: auto,
                               scaled to the canvas)
        --body <n>             Body size, glow radius in dots (default: auto)
        --theme <auto|off|dir> Follow the active Omarchy theme (default: auto),
                               force the built-in palette, or read a given
                               theme directory

TIMING (seconds)
        --sim <s>              Starfield phase      (default: 8)
        --morph <s>            Collapse into art    (default: 2.6)
        --hold <s>             Hold the art         (default: 5)
        --scatter <s>          Fly apart            (default: 1.6)
        --fps <n>              Frame rate           (default: 60)

OTHER
        --once                 Run one cycle and exit
        --managed              Do not read input; the launcher handles exit
    -h, --help                 This help

CONFIG
    ~/.config/omarchy-screensavers/config -- one option per line, # for comments.
    A value is the rest of its line, so `--text MY NAME` needs no quoting.
    Command-line options override it.
";

struct Opts {
    stars: Option<usize>,
    star_size: f64,
    speed: f64,
    warp: f64,
    black_hole: bool,
    hole_size: f64,
    file: Option<String>,
    text: Option<String>,
    text_scale: usize,
    art_scale: Option<usize>,
    stroke: Option<f64>,
    body: Option<f64>,
    theme: String,
    sim: f64,
    morph: f64,
    hold: f64,
    scatter: f64,
    fps: f64,
    once: bool,
    managed: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            stars: None,
            star_size: 1.2,
            speed: 2.0,
            warp: 1.0,
            black_hole: true,
            hole_size: 0.3,
            file: None,
            text: None,
            text_scale: 1,
            art_scale: None,
            stroke: None,
            body: None,
            theme: "auto".into(),
            sim: 8.0,
            morph: 2.6,
            hold: 5.0,
            scatter: 1.6,
            fps: 60.0,
            once: false,
            managed: false,
        }
    }
}

/// What a parse asked us to do instead of running.
enum Action {
    Run,
    Help,
    ShowPalette,
}

/// Applies `args` to `o`.
///
/// `strict` distinguishes the two sources. A mistake typed on the command line
/// should stop and say so. A mistake in the config file must not: the idle
/// timer launches this with no terminal to read an error from, and a stray
/// character in a config file is no reason for the screensaver to stop
/// appearing. Bad options from the file are reported and skipped.
fn apply(o: &mut Opts, args: &[String], strict: bool) -> Action {
    let reject = |what: String| -> bool {
        if strict {
            warn(format_args!("omarchy-screensaver-starry: {what}"));
            std::process::exit(2);
        }
        warn(format_args!("omarchy-screensaver-starry: ignoring config: {what}"));
        false
    };

    let mut i = 0;
    while i < args.len() {
        // Options that take a value; None means the value was missing.
        let value = |i: &mut usize, flag: &str| -> Option<String> {
            match args.get(*i + 1) {
                Some(v) => {
                    *i += 1;
                    Some(v.clone())
                }
                None => {
                    reject(format!("{flag} needs a value"));
                    None
                }
            }
        };
        let number = |i: &mut usize, flag: &str| -> Option<f64> {
            let v = value(i, flag)?;
            match v.trim().parse::<f64>() {
                Ok(n) => Some(n),
                Err(_) => {
                    reject(format!("{flag} expects a number, got {v:?}"));
                    None
                }
            }
        };

        match args[i].as_str() {
            "-h" | "--help" => return Action::Help,
            "--palette" => return Action::ShowPalette,
            "--stars" => {
                if let Some(v) = value(&mut i, "--stars") {
                    match v.trim().parse::<usize>() {
                        Ok(n) => o.stars = Some(n.clamp(50, 20_000)),
                        Err(_) => {
                            reject(format!("--stars expects a number, got {v:?}"));
                        }
                    }
                }
            }
            "--star-size" => {
                if let Some(n) = number(&mut i, "--star-size") {
                    o.star_size = n.clamp(0.0, 4.0);
                }
            }
            "--speed" => {
                if let Some(n) = number(&mut i, "--speed") {
                    o.speed = n.clamp(0.0, 10.0);
                }
            }
            "--warp" => {
                if let Some(n) = number(&mut i, "--warp") {
                    o.warp = n.clamp(0.0, 4.0);
                }
            }
            "--black-hole" => o.black_hole = true,
            "--no-black-hole" => o.black_hole = false,
            "--hole-size" => {
                if let Some(n) = number(&mut i, "--hole-size") {
                    o.hole_size = n.clamp(0.05, 0.8);
                }
            }
            "-f" | "--file" => {
                if let Some(v) = value(&mut i, "--file") {
                    o.file = Some(v);
                }
            }
            "-t" | "--text" => {
                if let Some(v) = value(&mut i, "--text") {
                    o.text = Some(v);
                }
            }
            "--art-scale" => {
                if let Some(v) = value(&mut i, "--art-scale") {
                    o.art_scale = match v.as_str() {
                        "auto" => None,
                        _ => match v.trim().parse::<usize>() {
                            Ok(n) => Some(n.max(1)),
                            Err(_) => {
                                reject(format!("--art-scale expects auto or a number, got {v:?}"));
                                o.art_scale
                            }
                        },
                    };
                }
            }
            // Read by omarchy-launch-screensavers: the font size it
            // starts the terminal at, and which screensaver it runs at all.
            // Accepted and ignored here so one config file serves both.
            "--font-size" => {
                let _ = value(&mut i, "--font-size");
            }
            "--screensaver" => {
                let _ = value(&mut i, "--screensaver");
            }
            // Poincare's options: accepted and ignored so one config file
            // serves every screensaver in the collection.
            "-o" | "--orbit" | "--layout" | "--periods" => {
                let flag = args[i].clone();
                let _ = value(&mut i, &flag);
            }
            "-l" | "--list" | "-V" | "--verify" => {}
            "--stroke" => {
                if let Some(n) = number(&mut i, "--stroke") {
                    o.stroke = Some(n.clamp(0.0, 12.0));
                }
            }
            "--body" => {
                if let Some(n) = number(&mut i, "--body") {
                    o.body = Some(n.clamp(1.0, 40.0));
                }
            }
            "--theme" => {
                if let Some(v) = value(&mut i, "--theme") {
                    o.theme = v;
                }
            }
            "--text-scale" => {
                if let Some(n) = number(&mut i, "--text-scale") {
                    o.text_scale = n.max(1.0) as usize;
                }
            }
            "--sim" => {
                if let Some(n) = number(&mut i, "--sim") {
                    o.sim = n;
                }
            }
            "--morph" => {
                if let Some(n) = number(&mut i, "--morph") {
                    o.morph = n;
                }
            }
            "--hold" => {
                if let Some(n) = number(&mut i, "--hold") {
                    o.hold = n;
                }
            }
            "--scatter" => {
                if let Some(n) = number(&mut i, "--scatter") {
                    o.scatter = n;
                }
            }
            "--fps" => {
                if let Some(n) = number(&mut i, "--fps") {
                    o.fps = n.clamp(5.0, 240.0);
                }
            }
            "--once" => o.once = true,
            "--managed" => o.managed = true,
            other => {
                let other = other.to_string();
                if strict {
                    warn(format_args!(
                        "omarchy-screensaver-starry: unknown option: {other}\n\n{HELP}"
                    ));
                    std::process::exit(2);
                }
                reject(format!("unknown option: {other}"));
            }
        }
        i += 1;
    }
    Action::Run
}

fn main() {
    // Rust ignores SIGPIPE and turns the resulting EPIPE into a panic, so
    // `omarchy-screensaver-starry --palette | head` dies noisily. Restore the
    // default and let the process end quietly the way every other CLI does.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    let mut o = Opts::default();

    // The config file is applied first so the command line can override it.
    // Any --help it might contain is ignored: those belong to whoever typed
    // the command.
    apply(&mut o, &config_args(), false);
    let cli: Vec<String> = std::env::args().skip(1).collect();
    let action = apply(&mut o, &cli, true);

    match action {
        Action::Help => {
            say!("{HELP}");
            return;
        }
        Action::ShowPalette => return show_palette(&o.theme),
        Action::Run => {}
    }

    if let Err(e) = run(o) {
        // The terminal is already restored by the guard at this point.
        //
        // Losing the terminal is the ordinary way this process ends: the idle
        // timer draws the screensaver, then the lock screen takes over and the
        // window goes with it. Every write after that fails, and that is a
        // normal shutdown rather than something to report.
        if terminal_is_gone(&e) {
            return;
        }
        warn(format_args!("omarchy-screensaver-starry: {e}"));
        std::process::exit(1);
    }
}

fn run(o: Opts) -> std::io::Result<()> {
    let art = StageArt::load(o.file.as_deref(), o.text.as_deref(), o.text_scale)?;

    let mut rng = Rng::from_clock();

    let stage = StageOpts {
        gather: o.sim,
        morph: o.morph,
        hold: o.hold,
        scatter: o.scatter,
        fps: o.fps,
        once: o.once,
        managed: o.managed,
        stroke: o.stroke,
        body: o.body,
        theme: o.theme.clone(),
        art_scale: o.art_scale,
        black_hole: o.black_hole,
        hole_size: o.hole_size,
    };

    let cfg = StarCfg { size: o.star_size, speed: o.speed, warp: o.warp };
    stage::run_stage(&stage, &art, &mut rng, o.managed, |rng, canvas| {
        let (dw, dh) = (canvas.dots_w() as f64, canvas.dots_h() as f64);
        Starfield::new(dw, dh, rng, o.stars, o.fps, cfg)
    })
}

fn show_palette(spec: &str) {
    use omarchy_screensaver_core::canvas::Rgb;
    let p = Palette::load(spec);
    let swatch = |c: Rgb| {
        format!(
            "\x1b[48;2;{};{};{}m      \x1b[0m {}",
            (c[0] * 255.0) as u8,
            (c[1] * 255.0) as u8,
            (c[2] * 255.0) as u8,
            palette::hex(c)
        )
    };
    sayln!("Source:     {}", p.source);
    match palette::theme_dir() {
        Some(d) => sayln!("Theme dir:  {}", d.display()),
        None => sayln!("Theme dir:  not found"),
    }
    sayln!();
    sayln!("background  {}", swatch(p.background));
    sayln!("logo        {}", swatch(p.logo_rest));
    for (i, b) in p.bodies.iter().enumerate() {
        sayln!("body {}      {}", i + 1, swatch(b.trail));
        sayln!("  core      {}", swatch(b.core));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Opts {
        let mut o = Opts::default();
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        apply(&mut o, &owned, false);
        o
    }

    #[test]
    fn options_are_applied() {
        let o = parse(&[
            "--sim", "3", "--fps", "30", "--stars", "800", "--star-size", "2",
            "--speed", "3", "--warp", "0.5", "--hole-size", "0.4", "--once",
        ]);
        assert_eq!(o.sim, 3.0);
        assert_eq!(o.fps, 30.0);
        assert_eq!(o.stars, Some(800));
        assert_eq!(o.star_size, 2.0);
        assert_eq!(o.speed, 3.0);
        assert_eq!(o.warp, 0.5);
        assert_eq!(o.hole_size, 0.4);
        assert!(o.once);
    }

    #[test]
    fn sky_and_hole_options_are_clamped() {
        assert_eq!(parse(&["--star-size", "99"]).star_size, 4.0);
        assert_eq!(parse(&["--speed", "-1"]).speed, 0.0);
        assert_eq!(parse(&["--warp", "99"]).warp, 4.0);
        assert_eq!(parse(&["--hole-size", "99"]).hole_size, 0.8);
        assert!(parse(&["--black-hole"]).black_hole);
        assert!(!parse(&["--black-hole", "--no-black-hole"]).black_hole);
    }

    #[test]
    fn stars_is_clamped_to_something_sane() {
        assert_eq!(parse(&["--stars", "3"]).stars, Some(50));
        assert_eq!(parse(&["--stars", "1000000"]).stars, Some(20_000));
    }

    #[test]
    fn a_broken_config_is_skipped_not_fatal() {
        let o = parse(&["--stars", "banana", "--nonsense", "--morph", "2.5"]);
        assert_eq!(o.stars, Opts::default().stars, "bad value should be ignored");
        assert_eq!(o.morph, 2.5, "later good options must still apply");
    }

    /// Poincare's options share the config file: they must not warn or eat
    /// the next option.
    #[test]
    fn poincare_options_are_accepted_and_ignored() {
        let o = parse(&[
            "--orbit", "figure-eight", "--layout", "grid", "--periods", "2", "--list", "--sim",
            "7",
        ]);
        assert_eq!(o.sim, 7.0);
    }

    /// Read by the launcher, not by the binary -- but they share one config
    /// file, so these must not produce warnings or eat the next option.
    #[test]
    fn launcher_only_options_are_accepted_and_ignored() {
        let o = parse(&["--font-size", "8", "--screensaver", "random", "--sim", "7"]);
        assert_eq!(o.sim, 7.0);
    }

    #[test]
    fn actions_are_recognised_and_stop_parsing() {
        let mut o = Opts::default();
        assert!(matches!(apply(&mut o, &["--palette".into()], false), Action::ShowPalette));
        assert!(matches!(apply(&mut o, &["-h".into()], false), Action::Help));
        assert!(matches!(apply(&mut o, &[], false), Action::Run));
    }

    #[test]
    fn every_option_in_the_help_text_is_actually_accepted() {
        for line in HELP.lines() {
            for word in line.split_whitespace() {
                let flag = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
                if !flag.starts_with("--") || flag.len() < 4 {
                    continue;
                }
                let mut o = Opts::default();
                // A rejected unknown option would exit; reaching here means it parsed.
                apply(&mut o, &[flag.to_string(), "1".to_string()], false);
            }
        }
    }
}
