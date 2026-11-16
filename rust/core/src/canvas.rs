/// An RGBA8888 pixel as the window buffer stores it: bytes R, G, B, A in memory, which on a
/// little-endian CPU (ARM Android, Apple silicon) reads as the `u32` 0xAABBGGRR.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color(pub u32);

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color(0xFF00_0000 | (b as u32) << 16 | (g as u32) << 8 | r as u32)
    }

    pub const fn r(self) -> u8 {
        self.0 as u8
    }
    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub const fn b(self) -> u8 {
        (self.0 >> 16) as u8
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }

    pub const fn right(&self) -> i32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub const fn inset(&self, d: i32) -> Rect {
        Rect::new(self.x + d, self.y + d, self.w - 2 * d, self.h - 2 * d)
    }
}

/// A view of a pixel buffer whose rows are `stride` pixels apart.
pub struct Canvas<'a> {
    px: &'a mut [u32],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> Canvas<'a> {
    pub fn new(px: &'a mut [u32], width: usize, height: usize, stride: usize) -> Self {
        assert!(stride >= width, "stride {stride} < width {width}");
        assert!(height == 0 || px.len() >= stride * (height - 1) + width, "buffer too small");
        Canvas { px, width, height, stride }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn fill(&mut self, c: Color) {
        for y in 0..self.height {
            self.row_mut(y).fill(c.0);
        }
    }

    pub fn fill_rect(&mut self, r: Rect, c: Color) {
        let Some((x0, y0, x1, y1)) = self.clip(r) else { return };
        for y in y0..y1 {
            self.row_mut(y)[x0..x1].fill(c.0);
        }
    }

    /// A `t`-pixel outline drawn inside `r`.
    pub fn stroke_rect(&mut self, r: Rect, t: i32, c: Color) {
        self.fill_rect(Rect::new(r.x, r.y, r.w, t), c);
        self.fill_rect(Rect::new(r.x, r.bottom() - t, r.w, t), c);
        self.fill_rect(Rect::new(r.x, r.y + t, t, r.h - 2 * t), c);
        self.fill_rect(Rect::new(r.right() - t, r.y + t, t, r.h - 2 * t), c);
    }

    /// Blends `c` over the pixel at (x, y) with coverage `a` (0–255). Out-of-bounds is ignored.
    pub fn blend(&mut self, x: i32, y: i32, c: Color, a: u8) {
        if a == 0 || x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let i = y as usize * self.stride + x as usize;
        if a == 255 {
            self.px[i] = c.0;
            return;
        }
        let d = Color(self.px[i]);
        let mix = |s: u8, d: u8| ((s as u32 * a as u32 + d as u32 * (255 - a as u32) + 127) / 255) as u8;
        self.px[i] = Color::rgb(mix(c.r(), d.r()), mix(c.g(), d.g()), mix(c.b(), d.b())).0;
    }

    /// Copies a `w`×`h` block of packed pixels with its top-left corner at (x, y), clipped.
    pub fn blit(&mut self, x: i32, y: i32, w: usize, h: usize, src: &[u32]) {
        let Some((x0, y0, x1, y1)) = self.clip(Rect::new(x, y, w as i32, h as i32)) else { return };
        let sx = (x0 as i32 - x) as usize;
        for row in y0..y1 {
            let sy = (row as i32 - y) as usize;
            let s = &src[sy * w + sx..sy * w + sx + (x1 - x0)];
            self.row_mut(row)[x0..x1].copy_from_slice(s);
        }
    }

    /// Copies rows into the canvas from a packed `width`-wide buffer of the same size.
    pub fn copy_from_packed(&mut self, src: &[u32]) {
        let w = self.width;
        for y in 0..self.height.min(src.len() / w.max(1)) {
            self.row_mut(y).copy_from_slice(&src[y * w..(y + 1) * w]);
        }
    }

    pub fn row_mut(&mut self, y: usize) -> &mut [u32] {
        let start = y * self.stride;
        &mut self.px[start..start + self.width]
    }

    pub fn pixel(&self, x: usize, y: usize) -> Color {
        Color(self.px[y * self.stride + x])
    }

    fn clip(&self, r: Rect) -> Option<(usize, usize, usize, usize)> {
        let x0 = r.x.max(0);
        let y0 = r.y.max(0);
        let x1 = r.right().min(self.width as i32);
        let y1 = r.bottom().min(self.height as i32);
        (x0 < x1 && y0 < y1).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_byte_order_is_rgba_in_memory() {
        let c = Color::rgb(0x11, 0x22, 0x33);
        assert_eq!(c.0.to_le_bytes(), [0x11, 0x22, 0x33, 0xFF]);
    }

    #[test]
    fn fill_rect_clips_and_respects_stride() {
        let mut px = vec![0u32; 6 * 4];
        let mut c = Canvas::new(&mut px, 4, 4, 6);
        c.fill_rect(Rect::new(-1, 2, 3, 5), Color(7));
        assert_eq!(c.pixel(0, 2), Color(7));
        assert_eq!(c.pixel(1, 3), Color(7));
        assert_eq!(c.pixel(2, 3), Color(0));
        assert_eq!(c.pixel(0, 1), Color(0));
        // Padding columns past the width are never written.
        assert!(px.chunks(6).all(|row| row[4] == 0 && row[5] == 0));
    }

    #[test]
    fn blend_mixes_channels() {
        let mut px = vec![Color::rgb(0, 0, 0).0];
        let mut c = Canvas::new(&mut px, 1, 1, 1);
        c.blend(0, 0, Color::rgb(255, 255, 255), 128);
        assert_eq!(c.pixel(0, 0), Color::rgb(128, 128, 128));
    }

    #[test]
    fn blit_clips_at_edges() {
        let mut px = vec![0u32; 9];
        let mut c = Canvas::new(&mut px, 3, 3, 3);
        c.blit(-1, 1, 2, 2, &[1, 2, 3, 4]);
        assert_eq!(c.pixel(0, 1), Color(2));
        assert_eq!(c.pixel(0, 2), Color(4));
        assert_eq!(c.pixel(1, 1), Color(0));
    }
}
