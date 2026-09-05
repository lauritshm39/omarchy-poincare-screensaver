//! Colours.
//!
//! By default these come from whatever Omarchy theme is currently applied.
//! `omarchy theme set` re-stages the active theme into a fixed directory, so
//! reading it at the start of every cycle means the screensaver follows a theme
//! change without being told about it.
//!
//! The fallback, used when no theme can be read, is the palette sampled from
//! the reference animation this was modelled on: amber, periwinkle and warm
//! white on black.

use crate::canvas::Rgb;
use std::path::{Path, PathBuf};

pub struct BodyColour {
    /// The glowing head.
    pub core: Rgb,
    /// The trail at full brightness, before age fades it toward the background.
    pub trail: Rgb,
}

pub struct Palette {
    pub background: Rgb,
    /// What the logo settles to once it has finished assembling.
    pub logo_rest: Rgb,
    pub bodies: [BodyColour; 3],
    /// For `--list`/diagnostics: where this palette came from.
    pub source: String,
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

/// Where `omarchy theme set` stages the active theme.
pub fn theme_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let dir = PathBuf::from(home).join(".local/state/omarchy/current/theme");
    dir.is_dir().then_some(dir)
}

impl Palette {
    /// Sampled from the reference animation. Used when there is no theme.
    pub fn reference() -> Palette {
        Palette {
            background: rgb(0, 0, 0),
            logo_rest: rgb(238, 232, 222),
            bodies: [
                BodyColour { core: rgb(255, 234, 190), trail: rgb(241, 154, 60) },
                BodyColour { core: rgb(223, 215, 246), trail: rgb(127, 131, 238) },
                BodyColour { core: rgb(255, 253, 242), trail: rgb(226, 210, 186) },
            ],
            source: "built-in".into(),
        }
    }

    /// `auto` follows the active Omarchy theme, `off` forces the built-in
    /// palette, anything else is read as a path to a theme directory or a
    /// colors.toml.
    pub fn load(spec: &str) -> Palette {
        match spec {
            "off" | "none" => Palette::reference(),
            "auto" => theme_dir()
                .and_then(|d| Palette::from_theme(&d))
                .unwrap_or_else(Palette::reference),
            path => Palette::from_theme(Path::new(path)).unwrap_or_else(|| {
                crate::warn(format_args!(
                    "omarchy-screensavers: no colours in {path:?}; using the built-in palette"
                ));
                Palette::reference()
            }),
        }
    }

    pub fn from_theme(dir: &Path) -> Option<Palette> {
        let file = if dir.is_dir() { dir.join("colors.toml") } else { dir.to_path_buf() };
        let text = std::fs::read_to_string(&file).ok()?;
        let mut p = Palette::from_theme_str(&text)?;
        p.source = match theme_name(&file) {
            Some(n) => format!("theme \"{n}\""),
            None => format!("{}", file.display()),
        };
        Some(p)
    }

    /// The colour derivation itself, independent of where the text came from.
    pub fn from_theme_str(text: &str) -> Option<Palette> {
        let colours = parse_colours(text);


        let get = |k: &str| colours.iter().find(|(n, _)| n == k).map(|(_, c)| *c);
        let background = get("background").or_else(|| get("bg"))?;
        // Not every theme declares a mode; the background says the same thing.
        let dark = if text.contains("mode = \"light\"") || text.contains("mode=\"light\"") {
            false
        } else if text.contains("mode = \"dark\"") || text.contains("mode=\"dark\"") {
            true
        } else {
            luminance(background) < 0.5
        };
        let logo_rest = get("bright_fg")
            .or_else(|| get("foreground"))
            .or_else(|| get("fg"))
            .unwrap_or(if dark { rgb(230, 230, 230) } else { rgb(30, 30, 30) });

        // Prefer a theme's bright accents on a dark background and its plain
        // ones on a light background, but accept either: some themes define
        // only one set.
        let order: &[&str] = if dark {
            &["bright_orange", "bright_yellow", "bright_red", "bright_magenta",
              "bright_blue", "bright_cyan", "bright_green",
              "orange", "yellow", "red", "magenta", "blue", "cyan", "green", "accent"]
        } else {
            &["orange", "yellow", "red", "magenta", "blue", "cyan", "green", "accent",
              "bright_orange", "bright_yellow", "bright_red", "bright_magenta",
              "bright_blue", "bright_cyan", "bright_green"]
        };
        let mut candidates: Vec<Rgb> = Vec::new();
        for key in order {
            if let Some(c) = get(key) {
                // Skip near-duplicates of something already taken.
                if candidates.iter().all(|p| hue_gap(*p, c) > 12.0 || saturation(c) < 0.05) {
                    candidates.push(c);
                }
            }
        }
        let chosen = pick_three(&candidates, background)?;

        let trails = shape(chosen, dark);
        let bodies = trails.map(|trail| BodyColour { core: mix(trail, logo_rest, 0.7), trail });

        Some(Palette { background, logo_rest, bodies, source: "theme".into() })
    }
}

/// The theme's display name, from the `theme.name` file beside its directory.
fn theme_name(colors_toml: &Path) -> Option<String> {
    let name = std::fs::read_to_string(
        colors_toml.parent()?.parent()?.join("theme.name"),
    )
    .ok()?;
    let name = name.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// Three colours as far apart in hue as the theme allows.
///
/// The bodies have to stay apart visually or the picture stops reading as three
/// of them, and a lot of themes are built from a narrow, low-saturation range.
/// Greedy maximum-separation gets the most out of whatever is there.
fn pick_three(candidates: &[Rgb], bg: Rgb) -> Option<[Rgb; 3]> {
    let usable: Vec<Rgb> = candidates
        .iter()
        .copied()
        .filter(|c| (luminance(*c) - luminance(bg)).abs() > 0.04)
        .collect();
    let pool = if usable.len() >= 3 { usable } else { candidates.to_vec() };
    if pool.is_empty() {
        return None;
    }

    let mut picked: Vec<Rgb> = Vec::with_capacity(3);
    // Start from the most saturated, which is the most recognisable hue.
    let first = pool
        .iter()
        .copied()
        .max_by(|a, b| saturation(*a).total_cmp(&saturation(*b)))?;
    picked.push(first);
    while picked.len() < 3 {
        let next = pool
            .iter()
            .copied()
            .filter(|c| !picked.iter().any(|p| p == c))
            .max_by(|a, b| {
                let score = |c: Rgb| {
                    let gap = picked.iter().map(|p| hue_gap(*p, c)).fold(f32::MAX, f32::min);
                    gap + saturation(c) * 30.0
                };
                score(*a).total_cmp(&score(*b))
            });
        match next {
            Some(c) => picked.push(c),
            // Fewer than three distinct colours: reuse, shifted in lightness so
            // the bodies are still told apart.
            None => {
                let base = picked[picked.len() % picked.len().max(1)];
                picked.push(shift_lightness(base, if picked.len() == 1 { 0.14 } else { -0.14 }));
            }
        }
    }
    Some([picked[0], picked[1], picked[2]])
}

/// Puts the three chosen colours into a lightness band where thin glowing
/// curves read clearly against the background, keeping each hue.
///
/// Themes are designed for text, not for this, and a fair number of them are
/// effectively monochrome -- `vantablack` and `white` have no usable hue
/// separation at all. When the hues are too close to tell apart, the bodies are
/// spread across lightness instead, which is the only axis such a theme leaves.
fn shape(chosen: [Rgb; 3], dark: bool) -> [Rgb; 3] {
    let monochrome = (0..3).any(|i| {
        (0..3).filter(|&j| j != i).any(|j| hue_gap(chosen[i], chosen[j]) < 20.0)
    });
    let (lo, hi) = match (dark, monochrome) {
        // A wider band when lightness is doing all the work.
        (true, true) => (0.40, 0.86),
        (true, false) => (0.52, 0.74),
        (false, true) => (0.18, 0.60),
        (false, false) => (0.30, 0.52),
    };

    let mut out = [[0.0; 3]; 3];
    for i in 0..3 {
        let (h, s, l) = to_hsl(chosen[i]);
        // A completely grey theme colour would otherwise make three identical
        // bodies even after the lightness spread.
        let s = s.max(0.08);
        let l = if monochrome {
            lo + (hi - lo) * [1.0, 0.55, 0.0][i]
        } else {
            l.clamp(lo, hi)
        };
        out[i] = from_hsl(h, s, l);
    }
    out
}

fn parse_colours(text: &str) -> Vec<(String, Rgb)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim();
        let Some(start) = rest.find('#') else { continue };
        let hex: String = rest[start + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if let Some(c) = from_hex(&hex) {
            out.push((key.to_string(), c));
        }
    }
    out
}

fn from_hex(hex: &str) -> Option<Rgb> {
    let v = match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok()?,
        3 => {
            let n = u32::from_str_radix(hex, 16).ok()?;
            let (r, g, b) = ((n >> 8) & 0xf, (n >> 4) & 0xf, n & 0xf);
            (r << 20) | (r << 16) | (g << 12) | (g << 8) | (b << 4) | b
        }
        _ => return None,
    };
    Some([
        ((v >> 16) & 0xff) as f32 / 255.0,
        ((v >> 8) & 0xff) as f32 / 255.0,
        (v & 0xff) as f32 / 255.0,
    ])
}

fn luminance(c: Rgb) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn saturation(c: Rgb) -> f32 {
    to_hsl(c).1
}

/// Shortest angular distance between two hues, in degrees, 0..180.
fn hue_gap(a: Rgb, b: Rgb) -> f32 {
    let d = (to_hsl(a).0 - to_hsl(b).0).abs() % 360.0;
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn shift_lightness(c: Rgb, by: f32) -> Rgb {
    let (h, s, l) = to_hsl(c);
    from_hsl(h, s, (l + by).clamp(0.0, 1.0))
}

fn to_hsl(c: Rgb) -> (f32, f32, f32) {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    ((h + 360.0) % 360.0, s.clamp(0.0, 1.0), l)
}

fn from_hsl(h: f32, s: f32, l: f32) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h % 360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c * 0.5;
    [
        (r + m).clamp(0.0, 1.0),
        (g + m).clamp(0.0, 1.0),
        (b + m).clamp(0.0, 1.0),
    ]
}

pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Hex for an OSC 11 background request.
pub fn hex(c: Rgb) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        (c[0] * 255.0 + 0.5) as u8,
        (c[1] * 255.0 + 0.5) as u8,
        (c[2] * 255.0 + 0.5) as u8
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(extra: &str) -> String {
        format!("mode = \"dark\"\nbackground = \"#1e1e2e\"\nbright_fg = \"#cdd6f4\"\n{extra}")
    }

    #[test]
    fn parses_hex_colours_and_ignores_comments() {
        let c = parse_colours("# red = \"#ff0000\"\nblue = \"#0000ff\"\nnope = notacolour\n");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].0, "blue");
        assert_eq!(from_hex("0000ff").unwrap(), c[0].1);
    }

    #[test]
    fn short_hex_expands() {
        assert_eq!(from_hex("f0a").unwrap(), from_hex("ff00aa").unwrap());
        assert!(from_hex("xyz").is_none());
        assert!(from_hex("12345").is_none());
    }

    #[test]
    fn hsl_round_trips() {
        for c in [[0.9, 0.3, 0.1], [0.1, 0.5, 0.9], [0.5, 0.5, 0.5], [0.0, 0.0, 0.0]] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-4, "{c:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn hue_gap_takes_the_short_way_round() {
        let red = from_hsl(10.0, 0.8, 0.5);
        let magenta = from_hsl(350.0, 0.8, 0.5);
        assert!((hue_gap(red, magenta) - 20.0).abs() < 1e-3);
        assert!(hue_gap(red, red) < 1e-3);
    }

    /// A theme with real accents must produce three visibly distinct bodies.
    #[test]
    fn distinct_accents_give_three_separated_hues() {
        let src = theme("red = \"#ea6962\"\ngreen = \"#a9b665\"\nblue = \"#7daea3\"\n");
        let p = Palette::from_theme_str(&src).expect("palette");
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            let gap = hue_gap(p.bodies[i].trail, p.bodies[j].trail);
            assert!(gap > 20.0, "bodies {i} and {j} only {gap} degrees apart");
        }
    }

    /// vantablack and white have no hue separation to give. The bodies must
    /// still be told apart -- they once all came out identical.
    #[test]
    fn monochrome_theme_separates_bodies_by_lightness() {
        for (mode, bg) in [("dark", "#000000"), ("light", "#ffffff")] {
            let src = format!(
                "mode = \"{mode}\"\nbackground = \"{bg}\"\nbright_fg = \"#888888\"\n\
                 red = \"#988282\"\ngreen = \"#8a8a8a\"\nblue = \"#909090\"\n"
            );
            let p = Palette::from_theme_str(&src).expect("palette");
            let mut ls: Vec<f32> = p.bodies.iter().map(|b| to_hsl(b.trail).2).collect();
            ls.sort_by(f32::total_cmp);
            assert!(
                ls[1] - ls[0] > 0.08 && ls[2] - ls[1] > 0.08,
                "{mode}: bodies not separated in lightness: {ls:?}"
            );
        }
    }

    /// A theme that declares no mode was once read as light, which inverted it.
    #[test]
    fn mode_is_inferred_from_the_background_when_undeclared() {
        let dark = Palette::from_theme_str(
            "background = \"#0a1428\"\nbright_fg = \"#ffffff\"\nred = \"#ff4eaa\"\n",
        )
        .unwrap();
        // On a dark background the bodies must be lighter than it.
        for b in &dark.bodies {
            assert!(
                luminance(b.trail) > luminance(dark.background),
                "body darker than a dark background"
            );
        }
        let light = Palette::from_theme_str(
            "background = \"#fffcf0\"\nbright_fg = \"#100f0f\"\nred = \"#c64386\"\n",
        )
        .unwrap();
        for b in &light.bodies {
            assert!(
                luminance(b.trail) < luminance(light.background),
                "body lighter than a light background"
            );
        }
    }

    #[test]
    fn a_theme_without_a_background_is_rejected() {
        assert!(Palette::from_theme_str("red = \"#ff0000\"\n").is_none());
    }

    #[test]
    fn built_in_palette_is_always_available() {
        let p = Palette::load("off");
        assert_eq!(p.source, "built-in");
        assert_eq!(p.bodies.len(), 3);
    }

    #[test]
    fn hex_formats_for_osc() {
        assert_eq!(hex([0.0, 0.0, 0.0]), "#000000");
        assert_eq!(hex([1.0, 1.0, 1.0]), "#ffffff");
    }
}
