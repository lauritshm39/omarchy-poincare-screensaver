//! A braille sub-cell canvas plus a diffing ANSI renderer.
//!
//! Each terminal cell holds a 2x4 grid of braille dots (U+2800..U+28FF). At the
//! font sizes Omarchy's screensaver terminals use those dots are very close to
//! square, so world coordinates map to dot coordinates with a single scale.
//! A cell carries one foreground colour, so the colours of the dots landing in
//! it are averaged.

use std::io::Write;

/// Bit for the dot at (x, y) within a cell, x in 0..2, y in 0..4.
const DOT_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

pub type Rgb = [f32; 3];

#[derive(Clone, Copy, PartialEq)]
struct Out {
    ch: char,
    fg: [u8; 3],
}

pub struct Canvas {
    pub cols: usize,
    pub rows: usize,
    bits: Vec<u8>,
    accum: Vec<Rgb>,
    weight: Vec<f32>,
    /// Explicit glyph placed over the braille layer ('\0' when unset).
    glyph: Vec<char>,
    glyph_fg: Vec<Rgb>,
}

impl Canvas {
    pub fn new(cols: usize, rows: usize) -> Self {
        let n = cols * rows;
        Canvas {
            cols,
            rows,
            bits: vec![0; n],
            accum: vec![[0.0; 3]; n],
            weight: vec![0.0; n],
            glyph: vec!['\0'; n],
            glyph_fg: vec![[0.0; 3]; n],
        }
    }

    pub fn dots_w(&self) -> usize {
        self.cols * 2
    }
    pub fn dots_h(&self) -> usize {
        self.rows * 4
    }

    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.accum.fill([0.0; 3]);
        self.weight.fill(0.0);
        self.glyph.fill('\0');
    }

    /// Light one dot. `intensity` scales the colour's contribution to the cell
    /// average, so faint trail tails do not drag a bright cell towards them.
    pub fn dot(&mut self, x: i32, y: i32, rgb: Rgb, intensity: f32) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= self.dots_w() || y >= self.dots_h() {
            return;
        }
        let i = (y / 4) * self.cols + (x / 2);
        self.bits[i] |= DOT_BITS[x % 2][y % 4];
        let w = intensity.max(0.0);
        for c in 0..3 {
            self.accum[i][c] += rgb[c] * w;
        }
        self.weight[i] += w;
    }

    /// Offsets of a filled disc of the given radius, for stamping at a point.
    /// Radius 0 is a single dot.
    pub fn disc(radius: i32) -> Vec<(i32, i32)> {
        let r = radius.max(0);
        let mut out = Vec::new();
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r + r {
                    out.push((dx, dy));
                }
            }
        }
        out
    }

    /// Bresenham line in dot space, `radius` dots thick.
    ///
    /// Trails are drawn as segments because a body near periapsis can crosses
    /// many dots between two sampled positions, and with thickness because a
    /// one-dot trail reads as far too faint once the canvas is fine.
    ///
    /// Thickness comes from stamping a bar perpendicular to the line's major
    /// axis at each step, not a disc: a disc is O(r^2) per step and almost
    /// entirely overlaps the previous step's, which at these trail lengths is
    /// most of the drawing cost and most of the bytes sent to the terminal.
    pub fn line(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        rgb: Rgb,
        intensity: f32,
        radius: i32,
    ) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        let x_major = dx >= -dy;
        let r = radius.max(0);
        // A pathological scale change could otherwise walk a huge span.
        let mut budget = (dx - dy) + 4;
        loop {
            for k in -r..=r {
                if x_major {
                    self.dot(x, y + k, rgb, intensity);
                } else {
                    self.dot(x + k, y, rgb, intensity);
                }
            }
            if (x == x1 && y == y1) || budget <= 0 {
                break;
            }
            budget -= 1;
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn glyph(&mut self, col: usize, row: usize, ch: char, rgb: Rgb) {
        if col >= self.cols || row >= self.rows {
            return;
        }
        let i = row * self.cols + col;
        self.glyph[i] = ch;
        self.glyph_fg[i] = rgb;
    }

    fn out(&self, i: usize) -> Out {
        if self.glyph[i] != '\0' {
            return Out { ch: self.glyph[i], fg: quantise(self.glyph_fg[i]) };
        }
        if self.bits[i] == 0 {
            return Out { ch: ' ', fg: [0, 0, 0] };
        }
        let w = self.weight[i].max(1e-6);
        let c = self.accum[i];
        Out {
            ch: char::from_u32(0x2800 + self.bits[i] as u32).unwrap_or(' '),
            fg: quantise([c[0] / w, c[1] / w, c[2] / w]),
        }
    }
}

fn quantise(c: Rgb) -> [u8; 3] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
    ]
}

/// Writes only the cells that changed since the previous frame. A full repaint
/// of a 4K terminal is several hundred KB; at 60fps that alone would keep a
/// core busy, which is the wrong thing for something that runs while idle.
pub struct Renderer {
    prev: Vec<Out>,
    buf: Vec<u8>,
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { prev: Vec::new(), buf: Vec::with_capacity(1 << 16) }
    }

    /// Forget the previous frame, forcing a full repaint (after a resize, or on
    /// the first frame).
    pub fn invalidate(&mut self) {
        self.prev.clear();
    }

    pub fn draw<W: Write>(&mut self, canvas: &Canvas, w: &mut W) -> std::io::Result<()> {
        let n = canvas.cols * canvas.rows;
        let blank = Out { ch: ' ', fg: [0, 0, 0] };
        let full = self.prev.len() != n;
        if full {
            self.prev = vec![blank; n];
            self.buf.clear();
            self.buf.extend_from_slice(b"\x1b[2J");
        } else {
            self.buf.clear();
        }

        let mut last_fg: Option<[u8; 3]>;
        for row in 0..canvas.rows {
            let mut col = 0;
            while col < canvas.cols {
                let i = row * canvas.cols + col;
                let cur = canvas.out(i);
                if !full && cur == self.prev[i] {
                    col += 1;
                    continue;
                }
                // Start a run. Cursor moves cost bytes too, so keep writing
                // across up to two unchanged cells rather than repositioning.
                write!(self.buf, "\x1b[{};{}H", row + 1, col + 1)?;
                last_fg = None;
                let mut skipped = 0;
                while col < canvas.cols {
                    let i = row * canvas.cols + col;
                    let cur = canvas.out(i);
                    let same = !full && cur == self.prev[i];
                    if same {
                        skipped += 1;
                        if skipped > 2 {
                            break;
                        }
                    } else {
                        skipped = 0;
                    }
                    if cur.ch == ' ' {
                        self.buf.push(b' ');
                        last_fg = None;
                    } else {
                        if last_fg != Some(cur.fg) {
                            write!(self.buf, "\x1b[38;2;{};{};{}m", cur.fg[0], cur.fg[1], cur.fg[2])?;
                            last_fg = Some(cur.fg);
                        }
                        let mut tmp = [0u8; 4];
                        self.buf.extend_from_slice(cur.ch.encode_utf8(&mut tmp).as_bytes());
                    }
                    self.prev[i] = cur;
                    col += 1;
                }
            }
        }

        if self.buf.is_empty() {
            return Ok(());
        }
        self.buf.extend_from_slice(b"\x1b[0m");
        w.write_all(&self.buf)?;
        w.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = [1.0, 1.0, 1.0];

    #[test]
    fn dots_map_to_the_right_braille_bits() {
        let mut c = Canvas::new(1, 1);
        c.dot(0, 0, WHITE, 1.0);
        assert_eq!(c.out(0).ch, '\u{2801}');
        c.dot(1, 3, WHITE, 1.0);
        assert_eq!(c.out(0).ch, char::from_u32(0x2800 + 0x81).unwrap());
    }

    #[test]
    fn an_empty_cell_is_a_space() {
        let c = Canvas::new(2, 2);
        assert_eq!(c.out(0).ch, ' ');
    }

    #[test]
    fn out_of_range_dots_are_discarded_not_wrapped() {
        let mut c = Canvas::new(2, 2);
        for (x, y) in [(-1, 0), (0, -1), (4, 0), (0, 8), (999, 999)] {
            c.dot(x, y, WHITE, 1.0);
        }
        for i in 0..4 {
            assert_eq!(c.out(i).ch, ' ', "cell {i} was written by an out-of-range dot");
        }
    }

    /// Intensity is what lets a bright body crossing its own faint tail still
    /// read as the bright thing in that cell.
    #[test]
    fn cell_colour_is_weighted_by_intensity() {
        let mut c = Canvas::new(1, 1);
        c.dot(0, 0, [1.0, 0.0, 0.0], 0.01);
        c.dot(0, 1, [0.0, 0.0, 1.0], 10.0);
        let fg = c.out(0).fg;
        assert!(fg[2] > 200 && fg[0] < 40, "expected the heavy blue to dominate, got {fg:?}");
    }

    #[test]
    fn clear_resets_everything() {
        let mut c = Canvas::new(2, 2);
        c.dot(0, 0, WHITE, 1.0);
        c.glyph(1, 1, 'X', WHITE);
        c.clear();
        for i in 0..4 {
            assert_eq!(c.out(i).ch, ' ');
        }
    }

    #[test]
    fn a_glyph_covers_the_braille_underneath() {
        let mut c = Canvas::new(1, 1);
        c.dot(0, 0, WHITE, 1.0);
        c.glyph(0, 0, '\u{2588}', WHITE);
        assert_eq!(c.out(0).ch, '\u{2588}');
    }

    #[test]
    fn a_line_connects_both_endpoints() {
        let mut c = Canvas::new(20, 5);
        c.line(0, 0, 30, 15, WHITE, 1.0, 0);
        assert_ne!(c.out(0).ch, ' ', "start not drawn");
        let last = (15 / 4) * 20 + (30 / 2);
        assert_ne!(c.out(last).ch, ' ', "end not drawn");
    }

    #[test]
    fn thickness_widens_a_line_without_moving_it() {
        let count = |radius: i32| {
            let mut c = Canvas::new(40, 10);
            c.line(2, 20, 76, 20, WHITE, 1.0, radius);
            (0..400).filter(|i| c.out(*i).ch != ' ').count()
        };
        let (thin, thick) = (count(0), count(2));
        assert!(thick > thin, "radius 2 ({thick}) should light more cells than 0 ({thin})");
    }

    #[test]
    fn a_zero_length_line_still_draws_a_dot() {
        let mut c = Canvas::new(2, 2);
        c.line(1, 1, 1, 1, WHITE, 1.0, 0);
        assert_ne!(c.out(0).ch, ' ');
    }

    #[test]
    fn disc_radius_zero_is_a_single_dot() {
        assert_eq!(Canvas::disc(0), [(0, 0)]);
        assert!(Canvas::disc(2).len() > 5);
        assert!(Canvas::disc(3).iter().all(|(x, y)| x * x + y * y <= 12));
    }

    /// The renderer must emit the cells that changed and nothing when nothing
    /// did, or an idle screensaver would keep a core busy.
    #[test]
    fn renderer_writes_the_first_frame_and_then_only_changes() {
        let mut c = Canvas::new(10, 3);
        let mut r = Renderer::new();
        let mut buf = Vec::new();

        c.dot(0, 0, WHITE, 1.0);
        r.draw(&c, &mut buf).unwrap();
        assert!(!buf.is_empty(), "first frame should paint");

        buf.clear();
        r.draw(&c, &mut buf).unwrap();
        assert!(buf.is_empty(), "an unchanged frame should emit nothing");

        buf.clear();
        c.dot(4, 2, WHITE, 1.0);
        r.draw(&c, &mut buf).unwrap();
        assert!(!buf.is_empty(), "a changed frame should emit something");
    }

    #[test]
    fn invalidate_forces_a_full_repaint() {
        let mut c = Canvas::new(4, 2);
        let mut r = Renderer::new();
        let mut buf = Vec::new();
        c.dot(0, 0, WHITE, 1.0);
        r.draw(&c, &mut buf).unwrap();
        buf.clear();
        r.invalidate();
        r.draw(&c, &mut buf).unwrap();
        assert!(!buf.is_empty(), "invalidate should force a repaint");
    }

    #[test]
    fn clearing_a_cell_erases_it_on_screen() {
        let mut c = Canvas::new(6, 2);
        let mut r = Renderer::new();
        let mut buf = Vec::new();
        c.dot(2, 0, WHITE, 1.0);
        r.draw(&c, &mut buf).unwrap();
        buf.clear();
        c.clear();
        r.draw(&c, &mut buf).unwrap();
        let out = String::from_utf8_lossy(&buf);
        assert!(out.contains(' '), "erasing a cell must write a space, got {out:?}");
    }
}
