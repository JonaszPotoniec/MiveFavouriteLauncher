//! What the two screens look like: the 3×4 grid and the per-slot app picker.

use crate::canvas::{Canvas, Color, Rect};
use crate::icon::Icon;
use crate::layout::{grid_cells, KEY_LABELS, SLOTS};
use crate::text::{Align, Text};

pub const BG: Color = Color::rgb(0x10, 0x14, 0x18);
const CELL: Color = Color::rgb(0x1f, 0x26, 0x2e);
const CELL_EMPTY: Color = Color::rgb(0x16, 0x1b, 0x21);
const TEXT: Color = Color::rgb(0xe8, 0xee, 0xf4);
const DIM: Color = Color::rgb(0x6b, 0x77, 0x84);
const ACCENT: Color = Color::rgb(0xff, 0xb5, 0x47);
const SELECTED: Color = Color::rgb(0x2b, 0x35, 0x40);

/// Converts dp to pixels for the screen density.
#[derive(Clone, Copy, Debug)]
pub struct Dp(pub f32);

impl Dp {
    pub fn from_density(dpi: u32) -> Dp {
        Dp(dpi.max(80) as f32 / 160.0)
    }
    pub fn px(self, dp: f32) -> i32 {
        (dp * self.0).round() as i32
    }
    pub fn pxf(self, dp: f32) -> f32 {
        dp * self.0
    }
}

/// Grid geometry: cells in keypad order and the icon size that fits them.
#[derive(Clone, Copy, Debug)]
pub struct GridLayout {
    pub cells: [Rect; SLOTS],
    pub icon: i32,
    dp: Dp,
}

impl GridLayout {
    pub fn new(area: Rect, dp: Dp) -> Self {
        let cells = grid_cells(area, dp.px(6.0));
        let c = cells[0];
        let icon = (c.w - dp.px(24.0)).min(c.h - dp.px(56.0)).clamp(dp.px(24.0), dp.px(72.0));
        GridLayout { cells, icon, dp }
    }
}

/// One slot as drawn: its label and icon, or `None` for an empty slot.
pub struct SlotView<'a> {
    pub label: &'a str,
    pub icon: Option<&'a Icon>,
}

pub fn draw_grid(canvas: &mut Canvas, layout: &GridLayout, text: &mut Text, slots: &[Option<SlotView>; SLOTS]) {
    let dp = layout.dp;
    canvas.fill(BG);
    let key_px = dp.pxf(15.0);
    let label_px = dp.pxf(12.5);
    for (i, cell) in layout.cells.iter().enumerate() {
        let slot = slots[i].as_ref();
        canvas.fill_rect(*cell, if slot.is_some() { CELL } else { CELL_EMPTY });
        let key = KEY_LABELS[i].to_string();
        let key_base = cell.y + dp.px(5.0) + text.ascent(key_px).round() as i32;
        text.draw(canvas, &key, cell.x + dp.px(7.0), key_base, key_px, ACCENT, Align::Left, cell.w);

        let cx = cell.x + cell.w / 2;
        match slot {
            Some(s) => {
                let label_h = dp.px(18.0);
                let block = layout.icon + dp.px(6.0) + label_h;
                let top = cell.y + (cell.h - block) / 2 + dp.px(4.0);
                if let Some(icon) = s.icon {
                    let off = (layout.icon - icon.size as i32) / 2;
                    icon.draw(canvas, cx - layout.icon / 2 + off, top + off);
                }
                let base = top + layout.icon + dp.px(6.0) + text.ascent(label_px).round() as i32;
                text.draw(canvas, s.label, cx, base, label_px, TEXT, Align::Center, cell.w - dp.px(10.0));
            }
            None => {
                let px = dp.pxf(28.0);
                let base = cell.y + cell.h / 2 + (text.ascent(px) / 2.0).round() as i32;
                text.draw(canvas, "+", cx, base, px, DIM, Align::Center, cell.w);
            }
        }
    }
}

pub fn draw_focus(canvas: &mut Canvas, layout: &GridLayout, i: usize) {
    if let Some(cell) = layout.cells.get(i) {
        canvas.stroke_rect(*cell, layout.dp.px(3.0), ACCENT);
    }
}

pub struct PickerItem {
    /// `None` is the "empty slot" row.
    pub component: Option<String>,
    pub label: String,
}

/// The app list shown when a slot is being assigned.
pub struct Picker {
    pub slot: usize,
    pub items: Vec<PickerItem>,
    pub selected: usize,
    pub top: usize,
}

pub struct PickerLayout {
    pub header: Rect,
    pub list: Rect,
    pub row_h: i32,
    pub icon: i32,
    dp: Dp,
}

impl PickerLayout {
    pub fn new(area: Rect, dp: Dp) -> Self {
        let header_h = dp.px(44.0);
        PickerLayout {
            header: Rect::new(area.x, area.y, area.w, header_h),
            list: Rect::new(area.x, area.y + header_h, area.w, area.h - header_h),
            row_h: dp.px(52.0),
            icon: dp.px(36.0),
            dp,
        }
    }

    pub fn visible_rows(&self) -> usize {
        (self.list.h / self.row_h).max(1) as usize
    }

    /// Item index under a touch at screen y, given the picker's scroll position.
    pub fn row_at(&self, picker: &Picker, y: i32) -> Option<usize> {
        if y < self.list.y || y >= self.list.bottom() {
            return None;
        }
        let i = picker.top + ((y - self.list.y) / self.row_h) as usize;
        (i < picker.items.len()).then_some(i)
    }
}

impl Picker {
    pub fn new(slot: usize, apps: Vec<PickerItem>, current: Option<&str>) -> Picker {
        let mut items = vec![PickerItem { component: None, label: "Empty slot".into() }];
        items.extend(apps);
        let selected = current
            .and_then(|c| items.iter().position(|it| it.component.as_deref() == Some(c)))
            .unwrap_or(0);
        Picker { slot, items, selected, top: 0 }
    }

    /// Moves the selection, wrapping at both ends, and scrolls it into view.
    pub fn step(&mut self, delta: isize, rows: usize) {
        let n = self.items.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(n) as usize;
        self.scroll_into_view(rows);
    }

    /// Moves by a page without wrapping.
    pub fn page(&mut self, pages: isize, rows: usize) {
        let n = self.items.len() as isize;
        self.selected = (self.selected as isize + pages * rows as isize).clamp(0, n - 1) as usize;
        self.scroll_into_view(rows);
    }

    pub fn scroll_into_view(&mut self, rows: usize) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + rows {
            self.top = self.selected + 1 - rows;
        }
    }

    /// Indices of the rows on screen.
    pub fn visible(&self, rows: usize) -> std::ops::Range<usize> {
        self.top..(self.top + rows).min(self.items.len())
    }
}

/// Draws the picker. `icon_for` returns the icon for a visible item, loading it if needed.
pub fn draw_picker<'i>(
    canvas: &mut Canvas,
    layout: &PickerLayout,
    text: &mut Text,
    picker: &Picker,
    mut icon_for: impl FnMut(&str) -> Option<&'i Icon>,
) {
    let dp = layout.dp;
    canvas.fill(BG);
    let h = layout.header;
    let title_px = dp.pxf(17.0);
    let title = format!("Key {}", KEY_LABELS[picker.slot]);
    let base = h.y + (h.h + text.ascent(title_px).round() as i32) / 2 - dp.px(2.0);
    text.draw(canvas, &title, h.x + dp.px(14.0), base, title_px, ACCENT, Align::Left, h.w);
    canvas.fill_rect(Rect::new(h.x, h.bottom() - 1, h.w, 1), CELL);

    let label_px = dp.pxf(15.0);
    let rows = layout.visible_rows();
    for (n, i) in picker.visible(rows).enumerate() {
        let item = &picker.items[i];
        let row = Rect::new(layout.list.x, layout.list.y + n as i32 * layout.row_h, layout.list.w, layout.row_h);
        if i == picker.selected {
            canvas.fill_rect(row, SELECTED);
            canvas.fill_rect(Rect::new(row.x, row.y, dp.px(4.0), row.h), ACCENT);
        }
        let ix = row.x + dp.px(14.0);
        let iy = row.y + (row.h - layout.icon) / 2;
        if let Some(icon) = item.component.as_deref().and_then(&mut icon_for) {
            let off = (layout.icon - icon.size as i32) / 2;
            icon.draw(canvas, ix + off, iy + off);
        }
        let tx = ix + layout.icon + dp.px(14.0);
        let base = row.y + (row.h + text.ascent(label_px).round() as i32) / 2 - dp.px(2.0);
        let color = if item.component.is_some() { TEXT } else { DIM };
        text.draw(canvas, &item.label, tx, base, label_px, color, Align::Left, row.right() - tx - dp.px(10.0));
    }
}

/// A full-screen one-line message, e.g. while the app list loads.
pub fn draw_message(canvas: &mut Canvas, area: Rect, dp: Dp, text: &mut Text, msg: &str) {
    canvas.fill(BG);
    let px = dp.pxf(16.0);
    text.draw(canvas, msg, area.x + area.w / 2, area.y + area.h / 2, px, DIM, Align::Center, area.w);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(n: usize) -> Vec<PickerItem> {
        (0..n).map(|i| PickerItem { component: Some(format!("p{i}/c")), label: format!("App {i}") }).collect()
    }

    #[test]
    fn picker_starts_on_current_app_and_wraps() {
        let mut p = Picker::new(4, apps(20), Some("p7/c"));
        assert_eq!(p.selected, 8); // row 0 is "Empty slot"
        p.scroll_into_view(5);
        assert_eq!(p.visible(5), 4..9);
        p.selected = 0;
        p.top = 0;
        p.step(-1, 5);
        assert_eq!(p.selected, 20);
        assert_eq!(p.top, 16);
        p.step(1, 5);
        assert_eq!((p.selected, p.top), (0, 0));
    }

    #[test]
    fn page_clamps() {
        let mut p = Picker::new(0, apps(3), None);
        p.page(5, 10);
        assert_eq!(p.selected, 3);
        p.page(-5, 10);
        assert_eq!(p.selected, 0);
    }
}
