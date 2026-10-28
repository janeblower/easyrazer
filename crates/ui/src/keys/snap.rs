//! Snap Tap groups: keys of which only one types at a time.

use std::collections::BTreeMap;

use easyrazer_backend::settings::SnapTap;
use egui::Color32;
use razer_core::rapid::{SnapGroup, SnapRule};

pub const RULES: [SnapRule; 3] = [SnapRule::Last, SnapRule::Neutral, SnapRule::Deeper];

/// Group colors, cycled; none is the accent, edited, error or driver color of the map.
const COLORS: [Color32; 4] = [Color32::from_rgb(0x3c, 0xc8, 0xe0), Color32::from_rgb(0xb4, 0x8c, 0xff), Color32::from_rgb(0xff, 0x7a, 0xb6), Color32::from_rgb(0x5b, 0x8c, 0xff)];

pub fn color(i: usize) -> Color32 {
    COLORS[i % COLORS.len()]
}

pub fn colors(s: &SnapTap) -> BTreeMap<u8, Color32> {
    s.groups.iter().enumerate().flat_map(|(i, g)| g.keys.iter().map(move |k| (*k, color(i)))).collect()
}

/// The new group takes its keys from the old ones; a group left with one key goes.
pub fn add_group(s: &SnapTap, keys: &[u8]) -> SnapTap {
    let mut groups: Vec<SnapGroup> = s
        .groups
        .iter()
        .map(|g| SnapGroup { keys: g.keys.iter().copied().filter(|k| !keys.contains(k)).collect(), rule: g.rule })
        .filter(|g| g.keys.len() > 1)
        .collect();
    groups.push(SnapGroup { keys: keys.to_vec(), rule: SnapRule::Last });
    SnapTap { enabled: s.enabled, groups }
}

pub fn remove_group(s: &SnapTap, i: usize) -> SnapTap {
    SnapTap { enabled: s.enabled, groups: s.groups.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, g)| g.clone()).collect() }
}

pub fn set_rule(s: &SnapTap, i: usize, rule: SnapRule) -> SnapTap {
    let mut next = s.clone();
    if let Some(g) = next.groups.get_mut(i) {
        g.rule = rule;
    }
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_group_takes_its_keys_and_drops_leftover_singles() {
        let s = SnapTap { enabled: true, groups: vec![SnapGroup { keys: vec![1, 2], rule: SnapRule::Deeper }, SnapGroup { keys: vec![3, 4, 5], rule: SnapRule::Last }] };
        let next = add_group(&s, &[2, 3]);
        assert_eq!(next.groups.len(), 2, "1 alone goes");
        assert_eq!(next.groups[0].keys, vec![4, 5]);
        assert_eq!(next.groups[1].keys, vec![2, 3]);
    }
}
