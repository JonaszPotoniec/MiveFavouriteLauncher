use crate::canvas::{Canvas, Color};

/// A square app icon as straight (not premultiplied) ARGB pixels, the layout Android's
/// `Bitmap.getPixels` returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon {
    pub size: usize,
    pub argb: Vec<u32>,
}

impl Icon {
    pub fn new(size: usize, argb: Vec<u32>) -> Option<Self> {
        (argb.len() == size * size && size > 0).then_some(Icon { size, argb })
    }

    /// Blends the icon over the canvas with its top-left corner at (x, y).
    pub fn draw(&self, canvas: &mut Canvas, x: i32, y: i32) {
        for row in 0..self.size {
            for col in 0..self.size {
                let p = self.argb[row * self.size + col];
                let a = (p >> 24) as u8;
                let c = Color::rgb((p >> 16) as u8, (p >> 8) as u8, p as u8);
                canvas.blend(x + col as i32, y + row as i32, c, a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_opaque_and_skips_transparent() {
        let icon = Icon::new(2, vec![0xFF_FF0000, 0x00_00FF00, 0xFF_0000FF, 0xFF_FFFFFF]).unwrap();
        let mut px = vec![Color::rgb(1, 2, 3).0; 4];
        let mut c = Canvas::new(&mut px, 2, 2, 2);
        icon.draw(&mut c, 0, 0);
        assert_eq!(c.pixel(0, 0), Color::rgb(255, 0, 0));
        assert_eq!(c.pixel(1, 0), Color::rgb(1, 2, 3));
        assert_eq!(c.pixel(0, 1), Color::rgb(0, 0, 255));
    }
}
