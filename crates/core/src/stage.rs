//! The shared phase machine: gather → collapse → hold → scatter → next round.
//!
//! Each screensaver provides the first phase -- poincare runs orbits, starry
//! drifts stars -- and everything after that is identical: the first phase's
//! dots become the sources of a [`Morph`] into the branding art, which holds,
//! settles to the theme foreground, and flies apart again.

use crate::canvas::{Canvas, Renderer, Rgb};
use crate::morph::Morph;
use crate::palette::{self, Palette};
use crate::rng::Rng;
use crate::{target, TermGuard};
use std::io::Write;
use std::time::{Duration, Instant};

/// The one thing each screensaver implements. `run_stage` drives the advance /
/// draw loop, then collects the sources and takes over with the shared
/// morph / hold / scatter phases.
pub trait GatherPhase {
    /// Run one frame of the opening phase.
    fn advance(&mut self);
    /// Paint the opening phase onto the canvas.
    fn draw(&self, canvas: &mut Canvas, pal: &Palette, style: Style);
    /// Every drawn dot, as morph sources.
    fn source_points(&self, out: &mut Vec<(f64, f64, Rgb)>, pal: &Palette);
}

/// How heavy the drawing is. Both scale with the canvas so that raising the
/// resolution makes the picture finer rather than merely thinner.
#[derive(Clone, Copy)]
pub struct Style {
    /// Radius of the trail brush, in dots. 0 is a single-dot line.
    pub stroke: i32,
    /// Radius of a body's glow, in dots.
    pub body: i32,
}

impl Style {
    pub fn auto(dots_h: usize, stroke: Option<f64>, body: Option<f64>) -> Style {
        let h = dots_h as f64;
        Style {
            stroke: stroke.unwrap_or((h / 170.0).clamp(1.0, 5.0)).round() as i32,
            body: body.unwrap_or((h / 28.0).clamp(4.0, 20.0)).round() as i32,
        }
    }
}

/// Timings and art shared by every screensaver.
pub struct StageOpts {
    /// Seconds in the opening (gather-source) phase.
    pub gather: f64,
    /// Seconds collapsing into the art.
    pub morph: f64,
    /// Seconds holding the art.
    pub hold: f64,
    /// Seconds flying apart.
    pub scatter: f64,
    pub fps: f64,
    pub once: bool,
    pub managed: bool,
    pub stroke: Option<f64>,
    pub body: Option<f64>,
    pub theme: String,
    pub art_scale: Option<usize>,
}

pub struct StageArt {
    pub source: target::Art,
}

impl StageArt {
    pub fn load(file: Option<&str>, text: Option<&str>, text_scale: usize) -> std::io::Result<StageArt> {
        let source = match (text, file) {
            (Some(t), _) => target::Art::from_text(t, text_scale),
            (None, Some(f)) => target::load(std::path::Path::new(f))?,
            (None, None) => target::load(&target::default_path())?,
        };
        if source.cells.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the morph target is empty -- nothing to assemble into",
            ));
        }
        Ok(StageArt { source })
    }
}

enum Phase {
    Gather,
    Morph,
    Hold,
    Scatter,
}

/// Run the phase machine. `build` constructs the opening phase fresh each
/// cycle (so e.g. poincare picks a new random orbit); `managed` is whether the
/// launcher owns keyboard input (true under `--managed`, where exit arrives as
/// a signal or a dead terminal instead of a keypress).
pub fn run_stage<G>(
    o: &StageOpts,
    art: &StageArt,
    rng: &mut Rng,
    managed: bool,
    mut build: impl FnMut(&mut Rng, &Canvas) -> G,
) -> std::io::Result<()>
where
    G: GatherPhase,
{
    let (_guard, mut out) = TermGuard::enter(managed)?;
    let raw_input = !managed && _guard.raw_enabled();

    let mut renderer = Renderer::new();
    let (mut cols, mut rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut canvas = Canvas::new(cols as usize, rows as usize);

    let frame = Duration::from_secs_f64(1.0 / o.fps);

    'outer: loop {
        // --- set up one cycle -------------------------------------------
        // Re-read every cycle: `omarchy theme set` re-stages the theme
        // directory, so this follows a theme change within one round.
        // Block art is enlarged to suit the terminal: at a small font the
        // logo would otherwise be a stamp in the middle of a much finer field.
        let scale =
            o.art_scale.unwrap_or_else(|| art.source.fit_scale(canvas.cols, canvas.rows));
        let scaled = art.source.scaled(scale);

        let style = Style::auto(canvas.dots_h(), o.stroke, o.body);
        let pal = Palette::load(&o.theme);
        out.write_all(format!("\x1b]11;{}\x07", palette::hex(pal.background)).as_bytes())?;
        renderer.invalidate();
        let mut gather = build(rng, &canvas);
        let mut phase = Phase::Gather;
        let mut morph: Option<Morph> = None;
        let mut t0 = Instant::now();
        // Per art cell: how far its best particle has got, and what colour it
        // arrived wearing.
        let mut arrival = vec![0.0f32; scaled.cells.len()];
        let mut arrival_colour = vec![Palette::reference().logo_rest; scaled.cells.len()];

        loop {
            let started = Instant::now();

            // --- input and resize ---------------------------------------
            if raw_input {
                while crossterm::event::poll(Duration::from_millis(0))? {
                    use crossterm::event::Event;
                    match crossterm::event::read()? {
                        Event::Resize(w, h) => {
                            cols = w;
                            rows = h;
                            canvas = Canvas::new(cols as usize, rows as usize);
                            renderer.invalidate();
                            continue 'outer;
                        }
                        Event::Key(_) | Event::Mouse(_) => break 'outer,
                        _ => {}
                    }
                }
            } else if let Ok((w, h)) = crossterm::terminal::size() {
                if (w, h) != (cols, rows) {
                    cols = w;
                    rows = h;
                    canvas = Canvas::new(cols as usize, rows as usize);
                    renderer.invalidate();
                    continue 'outer;
                }
            }

            canvas.clear();
            let t = t0.elapsed().as_secs_f64();

            match phase {
                Phase::Gather => {
                    gather.advance();
                    gather.draw(&mut canvas, &pal, style);
                    if t >= o.gather {
                        let mut src = Vec::new();
                        gather.source_points(&mut src, &pal);
                        morph = Some(make_morph(&canvas, &scaled, src, rng));
                        arrival.iter_mut().for_each(|a| *a = 0.0);
                        phase = Phase::Morph;
                        t0 = Instant::now();
                    }
                }
                Phase::Morph => {
                    let m = morph.as_ref().unwrap();
                    let p = (t / o.morph).min(1.0) as f32;
                    draw_particles(&mut canvas, m, p, 1.0, &mut arrival, &mut arrival_colour, &pal, style);
                    draw_art(&mut canvas, &scaled, &arrival, &arrival_colour, 0.0, &pal);
                    if t >= o.morph {
                        arrival.iter_mut().for_each(|a| *a = 1.0);
                        phase = Phase::Hold;
                        t0 = Instant::now();
                    }
                }
                Phase::Hold => {
                    // Settle from the colours the particles brought to a calm
                    // single tone, so it ends looking like the logo.
                    let settle = ((t / 1.4) as f32).clamp(0.0, 1.0);
                    draw_art(&mut canvas, &scaled, &arrival, &arrival_colour, settle, &pal);
                    if t >= o.hold {
                        morph = Some(make_scatter(&canvas, &scaled, rng, &pal));
                        phase = Phase::Scatter;
                        t0 = Instant::now();
                    }
                }
                Phase::Scatter => {
                    let m = morph.as_ref().unwrap();
                    let p = (t / o.scatter).min(1.0) as f32;
                    let mut ignore = vec![0.0f32; 0];
                    let mut ignore_c = vec![];
                    draw_particles(&mut canvas, m, p, 1.0 - p, &mut ignore, &mut ignore_c, &pal, style);
                    if t >= o.scatter {
                        if o.once {
                            break 'outer;
                        }
                        continue 'outer;
                    }
                }
            }

            renderer.draw(&canvas, &mut out)?;

            let spent = started.elapsed();
            if spent < frame {
                std::thread::sleep(frame - spent);
            }
        }
    }

    Ok(())
}

/// A body: white-hot centre with a small coloured halo. Braille gives one
/// colour per cell, so the halo has to be built from neighbouring dots.
/// A body: a solid bright core inside a soft halo of its own hue.
///
/// A single falloff curve does not work here. Braille gives one colour per
/// cell, so "brightness" is only ever a colour mixed toward the background, and
/// a smooth falloff spends most of its radius nearly invisible -- enlarging it
/// then adds halo nobody can see rather than a bigger body. A flat core with a
/// shorter falloff around it is what actually reads as a glowing point.
pub fn glow(c: &mut Canvas, x: f64, y: f64, col: &palette::BodyColour, bg: Rgb, r: i32) {
    let (cx, cy) = (x.round() as i32, y.round() as i32);
    let radius = r.max(1) as f32;
    let core = (radius * 0.42).max(1.0);

    for dy in -r..=r {
        for dx in -r..=r {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > radius {
                continue;
            }
            let t = if d <= core {
                1.0
            } else {
                (1.0 - (d - core) / (radius - core + 0.8)).clamp(0.0, 1.0).powf(1.5)
            };
            if t < 0.04 {
                continue;
            }
            let colour = palette::mix(col.trail, col.core, t);
            c.dot(
                cx + dx,
                cy + dy,
                palette::mix(bg, colour, 0.40 + 0.60 * t),
                // Weighted well above the trail so a body crossing its own tail
                // still reads as the bright thing in that cell.
                2.0 + t * 10.0,
            );
        }
    }
}

/// Where an art cell sits on screen, in dot coordinates.
pub fn art_origin(canvas: &Canvas, art: &target::Art) -> (f64, f64) {
    let col = (canvas.cols as f64 - art.width as f64) * 0.5;
    let row = (canvas.rows as f64 - art.height as f64) * 0.5;
    (col.max(0.0).floor(), row.max(0.0).floor())
}

pub fn make_morph(
    canvas: &Canvas,
    art: &target::Art,
    src: Vec<(f64, f64, Rgb)>,
    rng: &mut Rng,
) -> Morph {
    let (ox, oy) = art_origin(canvas, art);
    // Several particles per glyph, so the art assembles out of a cloud rather
    // than a sparse dusting -- but capped, or a small logo would need tens of
    // thousands of particles to consume the whole trail.
    let per_cell = (src.len() / art.cells.len().max(1)).clamp(1, 5);
    let mut targets = Vec::with_capacity(art.cells.len() * per_cell);
    for (idx, &(col, row, _)) in art.cells.iter().enumerate() {
        for _ in 0..per_cell {
            let x = (ox + col as f64) * 2.0 + rng.range(0.0, 2.0);
            let y = (oy + row as f64) * 4.0 + rng.range(0.0, 4.0);
            targets.push((x, y, idx));
        }
    }
    Morph::new(canvas.dots_w() as f64 * 0.5, canvas.dots_h() as f64 * 0.5, src, targets, rng)
}

pub fn make_scatter(canvas: &Canvas, art: &target::Art, rng: &mut Rng, pal: &Palette) -> Morph {
    let (ox, oy) = art_origin(canvas, art);
    let (dw, dh) = (canvas.dots_w() as f64, canvas.dots_h() as f64);
    let radius = (dw * dw + dh * dh).sqrt() * 0.7;
    let mut src = Vec::with_capacity(art.cells.len() * 3);
    let mut targets = Vec::with_capacity(art.cells.len() * 3);
    for &(col, row, _) in &art.cells {
        for _ in 0..3 {
            src.push((
                (ox + col as f64) * 2.0 + rng.range(0.0, 2.0),
                (oy + row as f64) * 4.0 + rng.range(0.0, 4.0),
                pal.logo_rest,
            ));
            let th = rng.range(0.0, std::f64::consts::TAU);
            targets.push((
                dw * 0.5 + radius * th.cos(),
                dh * 0.5 + radius * th.sin(),
                usize::MAX,
            ));
        }
    }
    Morph::new(dw * 0.5, dh * 0.5, src, targets, rng)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_particles(
    canvas: &mut Canvas,
    m: &Morph,
    p: f32,
    brightness: f32,
    arrival: &mut [f32],
    arrival_colour: &mut [Rgb],
    pal: &Palette,
    style: Style,
) {
    // Particles are the trail broken up, so they carry the same weight.
    let brush = Canvas::disc((style.stroke - 1).max(0));
    for particle in &m.particles {
        let (x, y, local) = m.at(particle, p);
        if particle.cell < arrival.len() {
            if local > arrival[particle.cell] {
                arrival[particle.cell] = local;
                arrival_colour[particle.cell] = particle.colour;
            }
            // Once its glyph is showing, the particle has done its job.
            if local >= 0.995 {
                continue;
            }
        }
        // Brighten on approach: the cloud arrives hot, then the glyphs take
        // over and cool to the resting colour.
        let b = brightness * (0.45 + 0.55 * local);
        let (px, py) = (x.round() as i32, y.round() as i32);
        let colour = palette::mix(pal.background, particle.colour, b);
        for &(bx, by) in &brush {
            canvas.dot(px + bx, py + by, colour, 0.6 + local);
        }
    }
}

pub fn draw_art(
    canvas: &mut Canvas,
    art: &target::Art,
    arrival: &[f32],
    arrival_colour: &[Rgb],
    settle: f32,
    pal: &Palette,
) {
    let (ox, oy) = art_origin(canvas, art);
    for (idx, &(col, row, ch)) in art.cells.iter().enumerate() {
        let a = arrival[idx];
        if a < 0.995 {
            continue;
        }
        let colour = palette::mix(arrival_colour[idx], pal.logo_rest, settle);
        canvas.glyph(ox as usize + col, oy as usize + row, ch, colour);
    }
}
