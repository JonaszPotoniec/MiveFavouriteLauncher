use crate::canvas::Rect;

pub const SLOTS: usize = 12;
pub const COLS: usize = 3;
pub const ROWS: usize = SLOTS / COLS;

/// Keypad labels in grid order: row by row, like the phone's keypad.
pub const KEY_LABELS: [char; SLOTS] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '*', '0', '#'];

/// Splits `area` into the 3×4 keypad grid with `gap` pixels around and between cells.
/// Leftover pixels from integer division go to the outer margin, so cells are equal.
pub fn grid_cells(area: Rect, gap: i32) -> [Rect; SLOTS] {
    let cols = COLS as i32;
    let rows = ROWS as i32;
    let cw = (area.w - gap * (cols + 1)) / cols;
    let ch = (area.h - gap * (rows + 1)) / rows;
    let ox = area.x + (area.w - (cw * cols + gap * (cols - 1))) / 2;
    let oy = area.y + (area.h - (ch * rows + gap * (rows - 1))) / 2;
    std::array::from_fn(|i| {
        let (c, r) = ((i % COLS) as i32, (i / COLS) as i32);
        Rect::new(ox + c * (cw + gap), oy + r * (ch + gap), cw, ch)
    })
}

/// Index of the cell under (x, y), if any.
pub fn cell_at(cells: &[Rect; SLOTS], x: i32, y: i32) -> Option<usize> {
    cells.iter().position(|r| r.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_are_equal_and_inside_area() {
        let area = Rect::new(0, 24, 480, 776);
        let cells = grid_cells(area, 8);
        for c in &cells {
            assert_eq!((c.w, c.h), (cells[0].w, cells[0].h));
            assert!(c.x >= area.x && c.right() <= area.right());
            assert!(c.y >= area.y && c.bottom() <= area.bottom());
        }
        // Keypad order: '5' is the centre of the second row.
        assert_eq!(cell_at(&cells, 240, cells[4].y + 5), Some(4));
        assert_eq!(KEY_LABELS[4], '5');
    }
}
