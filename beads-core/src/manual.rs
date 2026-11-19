//! Manual edit designs (the Edit Patterns tab): a necklace built item by item
//! from patterns 1-5 (forward or mirrored) and single beads.
//!
//! Pattern items are *links*: changing pattern 1 changes every place it is used,
//! just like the 2013 program.

use crate::pattern::{Pattern, MAX_PATTERNS};
use std::ops::Range;

/// Most items in one manual design (the 2013 limit).
pub const MAX_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualItem {
    /// Pattern number 0-4 (shown as 1-5); `mirrored` reverses its beads.
    Pattern { pattern: usize, mirrored: bool },
    /// One bead (index into the bead catalog).
    Bead(usize),
}

impl ManualItem {
    /// The beads this item puts on the strand, in order.
    pub fn beads(&self, patterns: &[Pattern; MAX_PATTERNS]) -> Vec<usize> {
        match *self {
            ManualItem::Pattern { pattern, mirrored } => {
                let mut b = patterns.get(pattern).map(Pattern::beads).unwrap_or_default();
                if mirrored {
                    b.reverse();
                }
                b
            }
            ManualItem::Bead(b) => vec![b],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManualDesign {
    pub items: Vec<ManualItem>,
}

impl ManualDesign {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// All the beads, item after item.
    pub fn strand(&self, patterns: &[Pattern; MAX_PATTERNS]) -> Vec<usize> {
        self.items.iter().flat_map(|i| i.beads(patterns)).collect()
    }

    /// Where each item's beads sit in `strand()`.
    pub fn item_ranges(&self, patterns: &[Pattern; MAX_PATTERNS]) -> Vec<Range<usize>> {
        let mut at = 0;
        self.items
            .iter()
            .map(|i| {
                let n = i.beads(patterns).len();
                at += n;
                at - n..at
            })
            .collect()
    }

    /// Adds an item at the end. Returns false when the design is full.
    pub fn push(&mut self, item: ManualItem) -> bool {
        self.insert(self.items.len(), item)
    }

    /// Inserts before position `at` (`at == len()` means the end).
    pub fn insert(&mut self, at: usize, item: ManualItem) -> bool {
        if self.items.len() >= MAX_ITEMS {
            return false;
        }
        self.items.insert(at.min(self.items.len()), item);
        true
    }

    pub fn replace(&mut self, i: usize, item: ManualItem) {
        if let Some(slot) = self.items.get_mut(i) {
            *slot = item;
        }
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.items.len() {
            self.items.remove(i);
        }
    }

    /// Flips a pattern item between forward and mirrored.
    pub fn flip(&mut self, i: usize) {
        if let Some(ManualItem::Pattern { mirrored, .. }) = self.items.get_mut(i) {
            *mirrored = !*mirrored;
        }
    }

    /// Moves item `i` one place left or right.
    pub fn move_item(&mut self, i: usize, left: bool) {
        let n = self.items.len();
        if left && i > 0 && i < n {
            self.items.swap(i, i - 1);
        } else if !left && i + 1 < n {
            self.items.swap(i, i + 1);
        }
    }

    /// Replaces a pattern item with its beads as single-bead items (2013 "Split a Pattern Apart").
    pub fn split(&mut self, i: usize, patterns: &[Pattern; MAX_PATTERNS]) {
        let Some(item) = self.items.get(i).copied() else { return };
        if matches!(item, ManualItem::Bead(_)) {
            return;
        }
        let beads: Vec<ManualItem> = item.beads(patterns).into_iter().map(ManualItem::Bead).collect();
        self.items.splice(i..=i, beads);
        self.items.truncate(MAX_ITEMS);
    }

    /// Splits every pattern item into single beads (2013 "Split ALL Patterns Apart").
    pub fn split_all(&mut self, patterns: &[Pattern; MAX_PATTERNS]) {
        let beads = self.strand(patterns);
        self.items = beads.into_iter().take(MAX_ITEMS).map(ManualItem::Bead).collect();
    }

    /// How many times pattern `p` (0-4) is used.
    pub fn count_pattern(&self, p: usize) -> usize {
        self.items.iter().filter(|i| matches!(i, ManualItem::Pattern { pattern, .. } if *pattern == p)).count()
    }

    /// Removes the last use of pattern `p` (the 2013 "-" button). Returns false if it isn't used.
    pub fn remove_last_pattern(&mut self, p: usize) -> bool {
        let pos = self.items.iter().rposition(|i| matches!(i, ManualItem::Pattern { pattern, .. } if *pattern == p));
        match pos {
            Some(i) => {
                self.items.remove(i);
                true
            }
            None => false,
        }
    }

    /// Describes an existing strand with as many whole patterns as possible
    /// (forward or mirrored), using single beads for anything left over.
    /// This is "Copy Main Design": the result draws exactly the same necklace.
    pub fn from_strand(strand: &[usize], patterns: &[Pattern; MAX_PATTERNS]) -> Self {
        // Longer patterns first; pattern 1 before 2; forward before mirrored.
        let mut candidates: Vec<(ManualItem, Vec<usize>)> = Vec::new();
        for p in 0..patterns.len() {
            for mirrored in [false, true] {
                let item = ManualItem::Pattern { pattern: p, mirrored };
                let beads = item.beads(patterns);
                if !beads.is_empty() && !candidates.iter().any(|(_, b)| *b == beads) {
                    candidates.push((item, beads));
                }
            }
        }
        candidates.sort_by_key(|(_, b)| std::cmp::Reverse(b.len()));

        let mut items = Vec::new();
        let mut i = 0;
        while i < strand.len() && items.len() < MAX_ITEMS {
            match candidates.iter().find(|(_, b)| strand[i..].starts_with(b)) {
                Some((item, b)) => {
                    items.push(*item);
                    i += b.len();
                }
                None => {
                    items.push(ManualItem::Bead(strand[i]));
                    i += 1;
                }
            }
        }
        ManualDesign { items }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pats() -> [Pattern; MAX_PATTERNS] {
        let mut p = [Pattern::default(); MAX_PATTERNS];
        p[0].set(0, 10);
        p[0].set(1, 11);
        p[0].set(3, 12); // a blank slot is skipped
        p[1].set(0, 20);
        p
    }

    #[test]
    fn items_expand_and_edit() {
        let p = pats();
        let mut m = ManualDesign::default();
        m.push(ManualItem::Pattern { pattern: 0, mirrored: false });
        m.push(ManualItem::Bead(7));
        m.push(ManualItem::Pattern { pattern: 0, mirrored: true });
        m.push(ManualItem::Pattern { pattern: 4, mirrored: false }); // empty pattern: no beads
        assert_eq!(m.strand(&p), vec![10, 11, 12, 7, 12, 11, 10]);
        assert_eq!(m.item_ranges(&p), vec![0..3, 3..4, 4..7, 7..7]);
        assert_eq!(m.count_pattern(0), 2);

        m.insert(1, ManualItem::Pattern { pattern: 1, mirrored: false });
        assert_eq!(m.strand(&p), vec![10, 11, 12, 20, 7, 12, 11, 10]);
        m.flip(0);
        assert_eq!(&m.strand(&p)[..3], &[12, 11, 10]);
        m.move_item(1, true);
        assert_eq!(m.items[0], ManualItem::Pattern { pattern: 1, mirrored: false });
        m.replace(2, ManualItem::Bead(9));
        assert_eq!(m.items[2], ManualItem::Bead(9));

        assert!(m.remove_last_pattern(0));
        assert_eq!(m.count_pattern(0), 1);
        m.split(1, &p); // pattern 1 (mirrored) -> 3 single beads
        assert_eq!(&m.items[1..4], &[ManualItem::Bead(12), ManualItem::Bead(11), ManualItem::Bead(10)]);
        let before = m.strand(&p);
        m.split_all(&p);
        assert_eq!(m.strand(&p), before);
        assert!(m.items.iter().all(|i| matches!(i, ManualItem::Bead(_))));
    }

    #[test]
    fn pattern_items_follow_pattern_changes() {
        let mut p = pats();
        let mut m = ManualDesign::default();
        m.push(ManualItem::Pattern { pattern: 1, mirrored: false });
        m.push(ManualItem::Pattern { pattern: 1, mirrored: false });
        p[1].set(1, 21);
        assert_eq!(m.strand(&p), vec![20, 21, 20, 21]);
    }

    #[test]
    fn copy_main_design_reproduces_the_strand() {
        let p = pats();
        // Two forward repeats, one extra bead, then mirrored repeats with one bead trimmed.
        let strand = vec![10, 11, 12, 10, 11, 12, 10, 20, 12, 11, 10, 11, 10];
        let m = ManualDesign::from_strand(&strand, &p);
        assert_eq!(m.strand(&p), strand);
        assert_eq!(
            m.items,
            vec![
                ManualItem::Pattern { pattern: 0, mirrored: false },
                ManualItem::Pattern { pattern: 0, mirrored: false },
                ManualItem::Bead(10),
                ManualItem::Pattern { pattern: 1, mirrored: false },
                ManualItem::Pattern { pattern: 0, mirrored: true },
                ManualItem::Bead(11),
                ManualItem::Bead(10),
            ]
        );
    }

    #[test]
    fn limit_is_respected() {
        let mut m = ManualDesign::default();
        for _ in 0..MAX_ITEMS {
            assert!(m.push(ManualItem::Bead(1)));
        }
        assert!(!m.push(ManualItem::Bead(1)));
    }
}
