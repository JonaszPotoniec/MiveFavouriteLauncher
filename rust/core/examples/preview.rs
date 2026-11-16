//! Renders the grid and the picker to PNGs at the phone's size, to check the layout without
//! the phone. Usage: cargo run -p mivefav-core --example preview -- <font.ttf> <out-dir>

use mivefav_core::canvas::{Canvas, Rect};
use mivefav_core::icon::Icon;
use mivefav_core::layout::SLOTS;
use mivefav_core::text::Text;
use mivefav_core::ui::{self, Dp, GridLayout, Picker, PickerItem, PickerLayout, SlotView};

// The Mive K-Suma: 480×800 at 220 dpi, status bar 24dp.
const W: usize = 480;
const H: usize = 800;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let font = std::fs::read(&args[1]).expect("font file");
    let out = std::path::Path::new(&args[2]);
    let mut text = Text::from_bytes(&font).unwrap();
    let dp = Dp::from_density(220);
    let area = Rect::new(0, dp.px(24.0), W as i32, H as i32 - dp.px(24.0));

    let layout = GridLayout::new(area, dp);
    let icon = disc(layout.icon as usize);
    let names = ["Telegram", "Messages", "Camera", "Very long application name", "Calculator", "", "Maps", "", "Music", "Settings", "Phone", ""];
    let views: [Option<SlotView>; SLOTS] =
        std::array::from_fn(|i| (!names[i].is_empty()).then(|| SlotView { label: names[i], icon: Some(&icon) }));
    let mut px = vec![0u32; W * H];
    let mut c = Canvas::new(&mut px, W, H, W);
    ui::draw_grid(&mut c, &layout, &mut text, &views);
    ui::draw_focus(&mut c, &layout, 4);
    write_png(&out.join("grid.png"), &px);

    let pl = PickerLayout::new(area, dp);
    let small = disc(pl.icon as usize);
    let apps = (0..30).map(|i| PickerItem { component: Some(format!("p{i}/c")), label: format!("Zażółć gęślą jaźń {i}") }).collect();
    let mut picker = Picker::new(4, apps, Some("p2/c"));
    picker.scroll_into_view(pl.visible_rows());
    let mut px = vec![0u32; W * H];
    ui::draw_picker(&mut Canvas::new(&mut px, W, H, W), &pl, &mut text, &picker, |_| Some(&small));
    write_png(&out.join("picker.png"), &px);
}

/// A stand-in icon: an anti-aliased filled circle.
fn disc(size: usize) -> Icon {
    let r = size as f32 / 2.0;
    let argb = (0..size * size)
        .map(|i| {
            let (x, y) = ((i % size) as f32 + 0.5 - r, (i / size) as f32 + 0.5 - r);
            let a = (r - (x * x + y * y).sqrt()).clamp(0.0, 1.0);
            ((a * 255.0) as u32) << 24 | 0x3d8bfd
        })
        .collect();
    Icon::new(size, argb).unwrap()
}

fn write_png(path: &std::path::Path, px: &[u32]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut enc = png::Encoder::new(file, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = px.iter().flat_map(|p| p.to_le_bytes()).collect();
    enc.write_header().unwrap().write_image_data(&bytes).unwrap();
}
