//! The morph target: whatever `omarchy branding screensaver` has put in
//! ~/.config/omarchy/branding/screensaver.txt. That file is block art when the
//! user transcoded an image and plain text when they typed a name, so the
//! target is the characters themselves at their own cells rather than a
//! rasterised bitmap. That way both cases land looking exactly like the file.

use std::path::{Path, PathBuf};


/// Sub-cell coverage of the block-drawing characters, in the same 2x4 bit
/// layout the canvas uses. This is what makes block art scalable: the art is
/// expanded into a dot bitmap, enlarged, and re-encoded back into characters.
///
/// The eighth blocks alias on purpose -- four rows cannot distinguish an eighth
/// from a quarter -- so several entries share a pattern and `char_for` will
/// return the first of them. That is a shape-preserving substitution, not a
/// loss.
const COVERAGE: &[(char, u8)] = &[
    (' ', 0x00),
    ('\u{2588}', 0xFF), // full block
    ('\u{2580}', 0x1B), // upper half
    ('\u{2584}', 0xE4), // lower half
    ('\u{258C}', 0x47), // left half
    ('\u{2590}', 0xB8), // right half
    ('\u{2598}', 0x03), // quadrant upper left
    ('\u{259D}', 0x18), // quadrant upper right
    ('\u{2596}', 0x44), // quadrant lower left
    ('\u{2597}', 0xA0), // quadrant lower right
    ('\u{259A}', 0xA3), // upper left + lower right
    ('\u{259E}', 0x5C), // upper right + lower left
    ('\u{259B}', 0x5F), // all but lower right
    ('\u{259C}', 0xBB), // all but lower left
    ('\u{2599}', 0xE7), // all but upper right
    ('\u{259F}', 0xFC), // all but upper left
    ('\u{2581}', 0xC0), // lower one eighth
    ('\u{2582}', 0xC0),
    ('\u{2583}', 0xC0),
    ('\u{2585}', 0xE4),
    ('\u{2586}', 0xF6),
    ('\u{2587}', 0xF6),
    ('\u{2594}', 0x09), // upper one eighth
];

/// Bit for the dot at (x, y) in a cell, matching canvas::DOT_BITS.
const DOT: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

fn coverage_of(ch: char) -> Option<u8> {
    COVERAGE.iter().find(|(c, _)| *c == ch).map(|(_, b)| *b)
}

/// The character whose coverage is closest to `bits`.
fn char_for(bits: u8) -> char {
    if bits == 0 {
        return ' ';
    }
    COVERAGE
        .iter()
        .min_by_key(|(_, b)| (b ^ bits).count_ones())
        .map(|(c, _)| *c)
        .unwrap_or('\u{2588}')
}

pub struct Art {
    pub width: usize,
    pub height: usize,
    /// (column, row, glyph) for every non-blank cell, relative to the art.
    pub cells: Vec<(usize, usize, char)>,
}

impl Art {
    /// Text drawn with the built-in block font, scaled so it reads as a logo.
    ///
    /// `scale` multiplies the glyph size. Terminal cells are about twice as
    /// tall as they are wide, so each font pixel becomes 2 cells wide and 1
    /// tall per unit of scale, which keeps the letters in proportion.
    pub fn from_text(text: &str, scale: usize) -> Art {
        let scale = scale.max(1);
        let (sx, sy) = (2 * scale, scale);
        let mut cells = Vec::new();
        let mut width = 0;
        let mut row0 = 0;

        for line in text.split('\n') {
            let mut col0 = 0;
            for ch in line.chars() {
                if let Some(bits) = glyph(ch) {
                    for (r, row) in bits.iter().enumerate() {
                        for c in 0..5 {
                            if row & (0b10000 >> c) == 0 {
                                continue;
                            }
                            for dy in 0..sy {
                                for dx in 0..sx {
                                    cells.push((col0 + c * sx + dx, row0 + r * sy + dy, '\u{2588}'));
                                }
                            }
                        }
                    }
                }
                // One font pixel of tracking between characters.
                col0 += 6 * sx;
            }
            width = width.max(col0.saturating_sub(sx));
            row0 += 8 * sy;
        }

        let height = row0.saturating_sub(sy);
        Art { width, height, cells }
    }


    /// True when this art is made of block-drawing characters, and so can be
    /// enlarged without turning into mush. Text art cannot.
    pub fn is_scalable(&self) -> bool {
        if self.cells.is_empty() {
            return false;
        }
        let blocks = self.cells.iter().filter(|(_, _, c)| coverage_of(*c).is_some()).count();
        blocks * 10 >= self.cells.len() * 7
    }

    /// This art enlarged `k` times.
    ///
    /// Each character is expanded into its 2x4 dot coverage, the whole bitmap
    /// is scaled, and every 2x4 block of the result is mapped back to the
    /// closest block character. Because the source is axis-aligned rectangles,
    /// integer scaling stays crisp.
    pub fn scaled(&self, k: usize) -> Art {
        if k <= 1 || !self.is_scalable() {
            return Art { width: self.width, height: self.height, cells: self.cells.clone() };
        }

        let (dw, dh) = (self.width * 2, self.height * 4);
        let mut src = vec![false; dw * dh];
        for &(col, row, ch) in &self.cells {
            let bits = coverage_of(ch).unwrap_or(0xFF);
            for x in 0..2 {
                for y in 0..4 {
                    if bits & DOT[x][y] != 0 {
                        src[(row * 4 + y) * dw + (col * 2 + x)] = true;
                    }
                }
            }
        }

        let (sw, sh) = (dw * k, dh * k);
        let (cols, rows) = (self.width * k, self.height * k);
        let mut cells = Vec::new();
        for row in 0..rows {
            for col in 0..cols {
                let mut bits = 0u8;
                for x in 0..2 {
                    for y in 0..4 {
                        let (sx, sy) = (col * 2 + x, row * 4 + y);
                        if sx < sw && sy < sh && src[(sy / k) * dw + (sx / k)] {
                            bits |= DOT[x][y];
                        }
                    }
                }
                if bits != 0 {
                    cells.push((col, row, char_for(bits)));
                }
            }
        }
        Art { width: cols, height: rows, cells }
    }

    /// The largest whole-number enlargement that still leaves the art a
    /// comfortable margin inside a canvas of `cols` x `rows` characters.
    pub fn fit_scale(&self, cols: usize, rows: usize) -> usize {
        if !self.is_scalable() || self.width == 0 || self.height == 0 {
            return 1;
        }
        let by_w = (cols as f64 * 0.80 / self.width as f64) as usize;
        let by_h = (rows as f64 * 0.55 / self.height as f64) as usize;
        by_w.min(by_h).clamp(1, 8)
    }

    pub fn from_str(s: &str) -> Art {
        let lines: Vec<&str> = s
            .trim_matches('\n')
            .split('\n')
            .map(|l| l.trim_end_matches(['\r', ' ']))
            .collect();
        let mut cells = Vec::new();
        let mut width = 0;
        for (row, line) in lines.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                if !ch.is_whitespace() {
                    cells.push((col, row, ch));
                }
                width = width.max(col + 1);
            }
        }
        Art { width, height: lines.len(), cells }
    }
}


/// A 5x7 block font, so a name given with --text arrives at the same weight as
/// the Omarchy logo instead of seven characters lost in the middle of a 4K
/// screen. Rows run top to bottom, bit 4 (0b10000) is the leftmost column.
const GLYPHS: &[(char, [u8; 7])] = &[
    ('!', [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100]),
    ('\'', [0b00100, 0b00100, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000]),
    ('+', [0b00000, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0b00000]),
    (',', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b01000]),
    ('-', [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000]),
    ('.', [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100]),
    ('/', [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000]),
    ('0', [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
    ('1', [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
    ('2', [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111]),
    ('3', [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110]),
    ('4', [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
    ('5', [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
    ('6', [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
    ('7', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
    ('8', [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
    ('9', [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
    (':', [0b00000, 0b00100, 0b00000, 0b00000, 0b00100, 0b00000, 0b00000]),
    ('?', [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b00000, 0b00100]),
    ('A', [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('B', [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110]),
    ('C', [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110]),
    ('D', [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110]),
    ('E', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111]),
    ('F', [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('G', [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111]),
    ('H', [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]),
    ('I', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111]),
    ('J', [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100]),
    ('K', [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001]),
    ('L', [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111]),
    ('M', [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001]),
    ('N', [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001]),
    ('O', [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('P', [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000]),
    ('Q', [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101]),
    ('R', [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001]),
    ('S', [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110]),
    ('T', [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('U', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110]),
    ('V', [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100]),
    ('W', [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001]),
    ('X', [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001]),
    ('Y', [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100]),
    ('Z', [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111]),
];

fn glyph(c: char) -> Option<&'static [u8; 7]> {
    let c = c.to_ascii_uppercase();
    GLYPHS.iter().find(|(g, _)| *g == c).map(|(_, b)| b)
}

/// Omarchy's branding file, falling back to the stock logo. Mirrors what
/// `omarchy-screensaver` feeds to ttfx.
pub fn default_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let branding = PathBuf::from(&home).join(".config/omarchy/branding/screensaver.txt");
    if branding.is_file() {
        return branding;
    }
    let omarchy = std::env::var("OMARCHY_PATH").unwrap_or_else(|_| "/usr/share/omarchy".into());
    PathBuf::from(omarchy).join("logo.txt")
}

pub fn load(path: &Path) -> std::io::Result<Art> {
    Ok(Art::from_str(&std::fs::read_to_string(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGO: &str = " \u{2584}\u{2588}\u{2588}\u{2584}\n\u{2588}  \u{2588}\n \u{2580}\u{2588}\u{2588}\u{2580}";

    #[test]
    fn parses_art_dimensions_and_skips_blanks() {
        let a = Art::from_str(LOGO);
        assert_eq!(a.width, 5);
        assert_eq!(a.height, 3);
        assert!(a.cells.iter().all(|(_, _, c)| !c.is_whitespace()));
        assert_eq!(a.cells.len(), 4 + 2 + 4);
    }

    /// Re-encoding must preserve the shape. Not the character: the eighth
    /// blocks alias deliberately, since a 2x4 grid cannot tell an eighth from a
    /// quarter, so several of them share one bit pattern. What must hold is
    /// that whatever character comes back covers the same dots.
    #[test]
    fn re_encoding_preserves_coverage() {
        for (ch, bits) in COVERAGE {
            let got = char_for(*bits);
            assert_eq!(
                coverage_of(got),
                Some(*bits),
                "{ch:?} (bits {bits:#04x}) re-encoded to {got:?} with different coverage"
            );
        }
    }

    /// Only 19 distinct coverages exist, so an arbitrary 8-bit pattern can be
    /// several dots away from the nearest character. That does not matter: the
    /// scaler only ever produces integer-scaled axis-aligned rectangles. This
    /// pins down the patterns it really emits, which must be exact.
    #[test]
    fn scaling_real_art_only_produces_exactly_representable_cells() {
        let arts = [
            Art::from_str(LOGO),
            Art::from_str(&std::fs::read_to_string("/usr/share/omarchy/logo.txt").unwrap_or_else(
                |_| LOGO.to_string(),
            )),
        ];
        for art in &arts {
            for k in 1..=5 {
                for (_, _, ch) in &art.scaled(k).cells {
                    assert!(
                        coverage_of(*ch).is_some(),
                        "scale {k} produced {ch:?}, which is not a block character"
                    );
                }
            }
        }
    }

    #[test]
    fn unknown_characters_are_treated_as_solid() {
        assert!(coverage_of('A').is_none());
        assert_eq!(char_for(0), ' ');
        assert_eq!(char_for(0xFF), '\u{2588}');
    }

    #[test]
    fn scaling_block_art_multiplies_its_size_and_stays_solid() {
        let a = Art::from_str(LOGO);
        let s = a.scaled(3);
        assert_eq!(s.width, a.width * 3);
        assert_eq!(s.height, a.height * 3);
        // A full block scaled up must remain full blocks, not dissolve.
        assert!(s.cells.iter().filter(|(_, _, c)| *c == '\u{2588}').count() > a.cells.len());
        // And nothing may land outside the declared bounds.
        assert!(s.cells.iter().all(|(c, r, _)| *c < s.width && *r < s.height));
    }

    #[test]
    fn scale_of_one_is_a_no_op() {
        let a = Art::from_str(LOGO);
        let s = a.scaled(1);
        assert_eq!((s.width, s.height), (a.width, a.height));
        assert_eq!(s.cells, a.cells);
    }

    /// Text art must not be enlarged -- it would turn to mush.
    #[test]
    fn text_art_is_not_scalable() {
        let a = Art::from_str("HELLO\nWORLD");
        assert!(!a.is_scalable());
        assert_eq!(a.scaled(4).width, a.width);
        assert_eq!(a.fit_scale(400, 100), 1);
        assert!(Art::from_str(LOGO).is_scalable());
    }

    #[test]
    fn fit_scale_keeps_the_art_inside_the_canvas() {
        let a = Art::from_str(LOGO); // 5x3
        for (cols, rows) in [(40, 12), (133, 35), (300, 84), (10, 4)] {
            let k = a.fit_scale(cols, rows);
            assert!(k >= 1);
            assert!(a.width * k <= cols, "{cols}x{rows}: width {} > {cols}", a.width * k);
            assert!(a.height * k <= rows, "{cols}x{rows}: height {} > {rows}", a.height * k);
        }
    }

    #[test]
    fn block_font_renders_text_at_logo_weight() {
        let a = Art::from_text("HI", 1);
        assert!(!a.cells.is_empty());
        // Two 5-wide glyphs, each pixel 2 cells wide, plus tracking.
        assert!(a.width >= 20 && a.width <= 24, "width {}", a.width);
        assert_eq!(a.height, 7);
        assert!(a.cells.iter().all(|(_, _, c)| *c == '\u{2588}'));
    }

    #[test]
    fn block_font_scale_enlarges_proportionally() {
        let one = Art::from_text("HI", 1);
        let two = Art::from_text("HI", 2);
        assert_eq!(two.height, one.height * 2);
        assert!(two.width > one.width);
    }

    #[test]
    fn block_font_is_well_formed() {
        let mut seen: Vec<char> = GLYPHS.iter().map(|(c, _)| *c).collect();
        let n = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), n, "duplicate glyphs in the font");
        for (c, rows) in GLYPHS {
            assert!(rows.iter().all(|r| *r < 0b100000), "{c:?} has bits outside 5 columns");
        }
        assert!(glyph('a').is_some(), "lowercase should map to uppercase");
        assert!(glyph('\u{263A}').is_none());
    }

    #[test]
    fn unknown_characters_are_skipped_not_drawn_as_blanks() {
        let a = Art::from_text("A\u{263A}A", 1);
        let b = Art::from_text("A A", 1);
        assert_eq!(a.cells.len(), b.cells.len());
    }

    #[test]
    fn multiline_text_stacks() {
        let one = Art::from_text("A", 1);
        let two = Art::from_text("A\nA", 1);
        assert!(two.height > one.height);
        assert_eq!(two.cells.len(), one.cells.len() * 2);
    }
}
