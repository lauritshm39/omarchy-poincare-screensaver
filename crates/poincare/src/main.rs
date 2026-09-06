//! omarchy-screensaver-poincare -- a three-body screensaver for Omarchy.
//!
//! Runs a periodic solution of the planar three-body problem in the terminal,
//! then collapses its trails into whatever `omarchy branding screensaver` has
//! set, holds, scatters, and moves on to the next orbit.

mod orbits;
mod physics;
mod sim;

use omarchy_screensaver_core::canvas::{Canvas, Rgb};
use omarchy_screensaver_core::config::config_args;
use omarchy_screensaver_core::palette::{self, Palette};
use omarchy_screensaver_core::rng::Rng;
use omarchy_screensaver_core::stage::{self, StageArt, StageOpts};
use omarchy_screensaver_core::{say, sayln, terminal_is_gone, warn};
use sim::Sim;

const HELP: &str = "\
omarchy-screensaver-poincare -- three-body screensaver for Omarchy

USAGE:
    omarchy-screensaver-poincare [OPTIONS]

ORBIT
    -o, --orbit <key|random>   Which simulation to run (default: random)
    -l, --list                 List the orbit library and exit
        --palette              Show the colours resolved from the current theme
    -V, --verify               Integrate every orbit for one period and report
                               how far it lands from its start, then exit
        --layout <single|grid> One orbit, or the whole library at once
                               (default: single)

TARGET
    -f, --file <path>          Art to morph into (default: Omarchy branding,
                               ~/.config/omarchy/branding/screensaver.txt)
    -t, --text <string>        Draw this text with the built-in block font
                               instead of reading a file
        --text-scale <n>       Enlarge --text by this factor (default: 1)
        --art-scale <auto|n>   Enlarge block art to suit the terminal size
                               (default: auto)

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
        --sim <s>              Orbit phase          (default: 10)
        --morph <s>            Collapse into art    (default: 2.6)
        --hold <s>             Hold the art         (default: 5)
        --scatter <s>          Fly apart            (default: 1.6)
        --periods <n>          Orbit periods drawn per sim phase (default: 2)
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
    orbit: Option<String>,
    grid: bool,
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
    periods: f64,
    fps: f64,
    once: bool,
    managed: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            orbit: None,
            grid: false,
            file: None,
            text: None,
            text_scale: 1,
            art_scale: None,
            stroke: None,
            body: None,
            theme: "auto".into(),
            sim: 10.0,
            morph: 2.6,
            hold: 5.0,
            scatter: 1.6,
            periods: 2.0,
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
    List,
    Verify,
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
            warn(format_args!("omarchy-screensaver-poincare: {what}"));
            std::process::exit(2);
        }
        warn(format_args!("omarchy-screensaver-poincare: ignoring config: {what}"));
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
            "-l" | "--list" => return Action::List,
            "-V" | "--verify" => return Action::Verify,
            "--palette" => return Action::ShowPalette,
            "-o" | "--orbit" => {
                if let Some(v) = value(&mut i, "--orbit") {
                    o.orbit = Some(v);
                }
            }
            "--layout" => {
                if let Some(v) = value(&mut i, "--layout") {
                    match v.as_str() {
                        "grid" => o.grid = true,
                        "single" => o.grid = false,
                        _ => {
                            reject(format!("--layout expects single or grid, got {v:?}"));
                        }
                    }
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
            // Starry's options: accepted and ignored so one config file
            // serves every screensaver in the collection.
            "--stars" | "--star-size" | "--speed" | "--warp" | "--hole-size" => {
                let _ = value(&mut i, "--stars");
            }
            "--black-hole" | "--no-black-hole" => {}
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
            "--periods" => {
                if let Some(n) = number(&mut i, "--periods") {
                    o.periods = n;
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
                        "omarchy-screensaver-poincare: unknown option: {other}\n\n{HELP}"
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
    // `omarchy-screensaver-poincare --list | head` dies noisily. Restore the default and
    // let the process end quietly the way every other CLI does.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    let mut o = Opts::default();

    // The config file is applied first so the command line can override it.
    // Any --help/--list/--verify it might contain is ignored: those belong to
    // whoever typed the command.
    apply(&mut o, &config_args(), false);
    let cli: Vec<String> = std::env::args().skip(1).collect();
    let action = apply(&mut o, &cli, true);

    match action {
        Action::Help => {
            say!("{HELP}");
            return;
        }
        Action::List => return list(),
        Action::Verify => return verify(),
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
        warn(format_args!("omarchy-screensaver-poincare: {e}"));
        std::process::exit(1);
    }
}


fn list() {
    sayln!("{:<16} {:<12} {:>10}  {}", "KEY", "FAMILY", "PERIOD", "NAME");
    for o in orbits::ORBITS {
        let mark = if orbits::is_reliable(o) { "" } else { "  *" };
        sayln!("{:<16} {:<12} {:>10.4}  {}{}", o.key, o.family, o.period, o.name, mark);
    }
    sayln!(
        "\n{} orbits, {} of them picked at random by default.",
        orbits::ORBITS.len(),
        orbits::reliable().len()
    );
    sayln!("Use --orbit <key>, or leave it out for random.");
    sayln!("* published initial conditions do not survive a full period; still");
    sayln!("  selectable by name. Run --verify for the measurements.");
}

fn verify() {
    // Defaults to the step ceiling the screensaver itself runs at, so what is
    // reported is what you will actually see. Raising it (or lowering it)
    // separates a bad transcription from an under-resolved close approach.
    let max_dt: f64 = std::env::var("OMARCHY_POINCARE_VERIFY_DT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(orbits::MAX_DT);

    sayln!("Integrating every orbit for exactly one period, with the adaptive");
    sayln!("stepper capped at dt = {max_dt:e}.\n");
    sayln!("  DRIFT   largest per-body displacement from the starting configuration");
    sayln!("          after one period, relative to that configuration's own size");
    sayln!("  ENERGY  relative change in total energy over the same integration");
    sayln!("  USABLE  fraction of a period before a body is flung away; the");
    sayln!("          screensaver replays only this much\n");
    sayln!(
        "{:<16} {:>9} {:>10} {:>9} {:>7} {:>13}  {}",
        "KEY", "PERIOD", "DRIFT", "ENERGY", "USABLE", "VIEW BOX", "VERDICT"
    );

    let mut disagreements = 0;
    for o in orbits::ORBITS {
        let start = o.system();
        let e0 = start.energy();
        let mut s = start.clone();
        let mut t = 0.0;
        while t < o.period {
            let dt = s.suggested_dt(max_dt, orbits::MIN_DT).min(o.period - t);
            s.step(dt);
            t += dt;
        }
        let scale = start.pos.iter().map(|p| p.norm()).fold(1e-9_f64, f64::max);
        let drift = (0..3)
            .map(|i| (s.pos[i] - start.pos[i]).norm() / scale)
            .fold(0.0_f64, f64::max);
        let eerr = ((s.energy() - e0) / e0.abs().max(1e-12)).abs();

        let prof = orbits::profile(o);
        let (bw, bh) = (prof.bbox.2 - prof.bbox.0, prof.bbox.3 - prof.bbox.1);
        let usable = prof.usable / o.period;

        // A drift of a few percent of the configuration size is not visible in
        // a curve a few hundred dots across; a drift of order the configuration
        // itself plainly is. Only the latter earns a place on UNRELIABLE.
        let good = drift < 1e-1 && usable > 0.99;
        if good != orbits::is_reliable(o) {
            disagreements += 1;
        }
        let verdict = match (good, orbits::is_reliable(o)) {
            (true, true) if drift < 1e-2 => "closes",
            (true, true) => "closes, with a drift too small to see",
            (false, false) => "does not close; excluded from random selection",
            (true, false) => "closes, but is marked unreliable -- update UNRELIABLE",
            (false, true) => "DOES NOT CLOSE but is not marked unreliable",
        };
        sayln!(
            "{:<16} {:>9.3} {:>10.2e} {:>9.1e} {:>6.0}% {:>6.2}x{:<6.2}  {}",
            o.key, o.period, drift, eerr, usable * 100.0, bw, bh, verdict
        );
    }

    sayln!();
    sayln!("`lagrange` and `euler` are derived in code rather than transcribed, so");
    sayln!("they close to machine precision and act as a check on the integrator.");
    sayln!("The rest come from Suvakov & Dmitrasinovic (PRL 110, 114301) at five or");
    sayln!("six significant digits, which the long-period members cannot survive.");
    if disagreements > 0 {
        sayln!("\n{disagreements} orbit(s) disagree with orbits::UNRELIABLE -- fix that list.");
        std::process::exit(1);
    }
}

fn show_palette(spec: &str) {
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

fn run(o: Opts) -> std::io::Result<()> {
    let art = StageArt::load(o.file.as_deref(), o.text.as_deref(), o.text_scale)?;

    let mut rng = Rng::from_clock();
    let chosen = match o.orbit.as_deref() {
        None | Some("random") => None,
        Some(k) => Some(orbits::find(k).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("unknown orbit {k:?}; try --list"),
            )
        })?),
    };

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
        black_hole: false,
        hole_size: 0.3,
    };

    stage::run_stage(&stage, &art, &mut rng, o.managed, |rng, canvas| {
        // Grid mode tiles the library; the stage runs one gather phase per
        // cycle, so combine the tiles into a single phase object.
        let mut sims = build_sims(&o, chosen, rng, canvas);
        let first = sims.remove(0);
        MultiSim { first, rest: sims }
    })
}

/// Several sims (grid mode) presented as one gather phase.
struct MultiSim {
    first: Sim,
    rest: Vec<Sim>,
}

impl stage::GatherPhase for MultiSim {
    fn advance(&mut self) {
        self.first.advance();
        for s in &mut self.rest {
            s.advance();
        }
    }

    fn draw(
        &self,
        canvas: &mut Canvas,
        pal: &Palette,
        style: stage::Style,
    ) {
        self.first.draw(canvas, pal, style);
        for s in &self.rest {
            s.draw(canvas, pal, style);
        }
    }

    fn source_points(&self, out: &mut Vec<(f64, f64, Rgb)>, pal: &Palette) {
        self.first.source_points(out, pal);
        for s in &self.rest {
            s.source_points(out, pal);
        }
    }
}

fn build_sims(
    o: &Opts,
    chosen: Option<&'static orbits::Orbit>,
    rng: &mut Rng,
    canvas: &Canvas,
) -> Vec<Sim> {
    let (dw, dh) = (canvas.dots_w() as f64, canvas.dots_h() as f64);
    // A trail is stored as distinct dots, so the number a closed curve needs
    // grows with the linear size of the canvas, not its area. Scaling the cap
    // keeps the whole curve visible at a small font instead of quietly
    // truncating its tail.
    let trail_budget = |span: f64| ((span * 60.0) as usize).clamp(12_000, 120_000);
    if !o.grid {
        let pool = orbits::reliable();
        let orbit = chosen.unwrap_or_else(|| pool[rng.below(pool.len())]);
        let cap = trail_budget(dw.max(dh));
        return vec![Sim::new(orbit, 0.0, 0.0, dw, dh, 0.22, o.periods, o.sim, o.fps, cap)];
    }

    // Lay the whole library out in a grid, as in the reference animation.
    let pool = orbits::reliable();
    let n = pool.len();
    let aspect = dw / dh.max(1.0);
    let mut cols = ((n as f64 * aspect).sqrt().round() as usize).max(1);
    let mut rows = n.div_ceil(cols);
    while cols * rows < n {
        cols += 1;
        rows = n.div_ceil(cols);
    }
    let (tw, th) = (dw / cols as f64, dh / rows as f64);
    let tile_cap = (trail_budget(tw.max(th)) / 4).clamp(3_000, 30_000);
    pool.iter()
        .enumerate()
        .map(|(k, orbit)| {
            let (cx, cy) = (k % cols, k / cols);
            Sim::new(
                *orbit,
                cx as f64 * tw,
                cy as f64 * th,
                tw,
                th,
                0.30,
                o.periods,
                o.sim,
                o.fps,
                tile_cap,
            )
        })
        .collect()
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
        let o = parse(&["--sim", "3", "--fps", "30", "--layout", "grid", "--once"]);
        assert_eq!(o.sim, 3.0);
        assert_eq!(o.fps, 30.0);
        assert!(o.grid);
        assert!(o.once);
    }

    #[test]
    fn later_options_win_so_the_command_line_overrides_the_config() {
        let mut o = Opts::default();
        apply(&mut o, &["--sim".into(), "5".into()], false);
        apply(&mut o, &["--sim".into(), "9".into()], true);
        assert_eq!(o.sim, 9.0);
    }

    /// A stray character in the config must never stop the screensaver: the
    /// idle timer launches it with no terminal to read an error from.
    #[test]
    fn a_broken_config_is_skipped_not_fatal() {
        let o = parse(&["--sim", "banana", "--nonsense", "--morph", "2.5"]);
        assert_eq!(o.sim, Opts::default().sim, "bad value should be ignored");
        assert_eq!(o.morph, 2.5, "later good options must still apply");
    }

    #[test]
    fn a_missing_value_at_the_end_is_survivable() {
        let o = parse(&["--fps"]);
        assert_eq!(o.fps, Opts::default().fps);
    }

    #[test]
    fn layout_rejects_anything_but_single_or_grid() {
        assert!(!parse(&["--layout", "sideways"]).grid);
        assert!(parse(&["--layout", "grid"]).grid);
        assert!(!parse(&["--layout", "grid", "--layout", "single"]).grid);
    }

    #[test]
    fn fps_is_clamped_to_something_sane() {
        assert!(parse(&["--fps", "100000"]).fps <= 240.0);
        assert!(parse(&["--fps", "0"]).fps >= 5.0);
    }

    /// Read by the launcher or the sibling binary, not by this one -- but they
    /// share one config file, so these must not produce warnings or eat the
    /// next option.
    #[test]
    fn launcher_only_options_are_accepted_and_ignored() {
        let o = parse(&[
            "--font-size", "8", "--screensaver", "random", "--stars", "800", "--star-size",
            "2", "--speed", "3", "--warp", "0.5", "--hole-size", "0.4", "--black-hole",
            "--sim", "7",
        ]);
        assert_eq!(o.sim, 7.0);
    }

    #[test]
    fn actions_are_recognised_and_stop_parsing() {
        let mut o = Opts::default();
        assert!(matches!(apply(&mut o, &["--list".into()], false), Action::List));
        assert!(matches!(apply(&mut o, &["-V".into()], false), Action::Verify));
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
