use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

use crate::canvas::{Canvas, Color};

const ELLIPSIS: char = '…';

/// A font with a cache of rasterized glyphs, keyed by character and pixel size.
pub struct Text {
    font: Font,
    glyphs: HashMap<(char, u32), (Metrics, Vec<u8>)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Center,
}

impl Text {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        let font = Font::from_bytes(bytes, FontSettings { scale: 40.0, ..FontSettings::default() })?;
        Ok(Text { font, glyphs: HashMap::new() })
    }

    /// Distance from the top of a line to its baseline, in pixels.
    pub fn ascent(&self, px: f32) -> f32 {
        self.font.horizontal_line_metrics(px).map_or(px * 0.8, |m| m.ascent)
    }

    pub fn width(&mut self, s: &str, px: f32) -> f32 {
        s.chars().map(|c| self.glyph(c, px).0.advance_width).sum()
    }

    /// Draws one line of `s` with its baseline at `baseline`. `x` is the left edge, or the
    /// centre for [`Align::Center`]. Text wider than `max_w` is cut and ends in an ellipsis.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        s: &str,
        x: i32,
        baseline: i32,
        px: f32,
        color: Color,
        align: Align,
        max_w: i32,
    ) {
        let fitted = self.fit(s, px, max_w as f32);
        let w = self.width(&fitted, px);
        let mut pen = match align {
            Align::Left => x as f32,
            Align::Center => x as f32 - w / 2.0,
        };
        for c in fitted.chars() {
            let (m, bitmap) = self.glyph(c, px);
            let gx = pen.round() as i32 + m.xmin;
            let gy = baseline - m.height as i32 - m.ymin;
            for row in 0..m.height {
                for col in 0..m.width {
                    let a = bitmap[row * m.width + col];
                    canvas.blend(gx + col as i32, gy + row as i32, color, a);
                }
            }
            pen += m.advance_width;
        }
    }

    fn fit(&mut self, s: &str, px: f32, max_w: f32) -> String {
        if self.width(s, px) <= max_w {
            return s.to_string();
        }
        let budget = max_w - self.glyph(ELLIPSIS, px).0.advance_width;
        let mut used = 0.0;
        let mut out = String::new();
        for c in s.chars() {
            let adv = self.glyph(c, px).0.advance_width;
            if used + adv > budget {
                break;
            }
            used += adv;
            out.push(c);
        }
        let mut out = out.trim_end().to_string();
        out.push(ELLIPSIS);
        out
    }

    fn glyph(&mut self, c: char, px: f32) -> &(Metrics, Vec<u8>) {
        let key = (c, (px * 4.0).round() as u32);
        self.glyphs.entry(key).or_insert_with(|| self.font.rasterize(c, px))
    }
}
