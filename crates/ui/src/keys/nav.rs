//! Moving over the key map with the arrows.

use razer_core::layout::LayoutKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

fn center(k: &LayoutKey) -> (f32, f32) {
    (k.x + k.w / 2.0, k.y + k.h / 2.0)
}

/// The key an arrow moves to: the caps are not a grid, so the nearest one that way wins,
/// keeping to the row or column.
pub fn step<'a>(keys: &[&'a LayoutKey], from: &LayoutKey, dir: Direction) -> Option<&'a LayoutKey> {
    let (fx, fy) = center(from);
    let horizontal = matches!(dir, Direction::Left | Direction::Right);
    let sign = if matches!(dir, Direction::Right | Direction::Down) { 1.0 } else { -1.0 };
    let mut best = None;
    let mut best_score = f32::INFINITY;
    for &k in keys {
        if k.key == from.key {
            continue;
        }
        let (x, y) = center(k);
        let along = (if horizontal { x - fx } else { y - fy }) * sign;
        if along <= 0.01 {
            continue;
        }
        let across = (if horizontal { y - fy } else { x - fx }).abs();
        // Straying from the row costs more than going further along it.
        let score = along + across * 2.0;
        if score < best_score {
            best = Some(k);
            best_score = score;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use razer_core::layout::Shape;

    fn cap(key: u8, x: f32, y: f32, w: f32) -> LayoutKey {
        LayoutKey { key, label: "", x, y, w, h: 1.0, editable: true, shape: Shape::Key }
    }

    // Tab Q W / Caps A S / a wide Shift under them.
    fn board() -> [LayoutKey; 7] {
        [cap(1, 0.0, 0.0, 1.5), cap(2, 1.5, 0.0, 1.0), cap(3, 2.5, 0.0, 1.0), cap(4, 0.0, 1.0, 1.75), cap(5, 1.75, 1.0, 1.0), cap(6, 2.75, 1.0, 1.0), cap(7, 0.0, 2.0, 3.75)]
    }

    fn go(from: u8, dir: Direction) -> Option<u8> {
        let b = board();
        let keys: Vec<&LayoutKey> = b.iter().collect();
        let from = b.iter().find(|k| k.key == from).unwrap();
        step(&keys, from, dir).map(|k| k.key)
    }

    #[test]
    fn arrows_keep_to_the_row() {
        assert_eq!(go(1, Direction::Right), Some(2));
        assert_eq!(go(2, Direction::Right), Some(3));
        assert_eq!(go(3, Direction::Left), Some(2));
        assert_eq!(go(5, Direction::Left), Some(4));
    }

    #[test]
    fn up_and_down_take_the_nearest_key_of_the_next_row() {
        assert_eq!(go(2, Direction::Down), Some(5));
        assert_eq!(go(3, Direction::Down), Some(6));
        assert_eq!(go(6, Direction::Up), Some(3));
        assert_eq!(go(5, Direction::Down), Some(7));
        assert_eq!(go(7, Direction::Up), Some(5));
    }

    #[test]
    fn nothing_past_the_edge() {
        assert_eq!(go(1, Direction::Left), None);
        assert_eq!(go(3, Direction::Up), None);
        assert_eq!(go(7, Direction::Down), None);
    }
}
