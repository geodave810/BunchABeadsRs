//! A pattern is 12 slots, each either empty or holding a bead (an index into the catalog).

use crate::rng::Rng;

pub const PATTERN_SIZE: usize = 12; // beads per pattern (same as 2013)
pub const MAX_PATTERNS: usize = 5; // patterns 3-5 are for manual edit designs

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pattern {
    pub slots: [Option<usize>; PATTERN_SIZE],
}

impl Pattern {
    pub fn get(&self, slot: usize) -> Option<usize> {
        self.slots[slot]
    }
    pub fn set(&mut self, slot: usize, bead: usize) {
        self.slots[slot] = Some(bead);
    }
    pub fn remove(&mut self, slot: usize) {
        self.slots[slot] = None;
    }
    pub fn clear(&mut self) {
        self.slots = [None; PATTERN_SIZE];
    }
    pub fn is_empty(&self) -> bool {
        self.bead_count() == 0
    }
    pub fn bead_count(&self) -> usize {
        self.slots.iter().flatten().count()
    }
    /// Beads in order, blank slots skipped.
    pub fn beads(&self) -> Vec<usize> {
        self.slots.iter().flatten().copied().collect()
    }

    /// Shift the selected slots one position left/right, swapping with the
    /// unselected neighbour (the old "<" and ">" buttons). `selected` is updated
    /// so the selection follows the beads that moved.
    pub fn shift_selected(&mut self, selected: &mut [bool; PATTERN_SIZE], right: bool) {
        if right {
            for i in (0..PATTERN_SIZE - 1).rev() {
                if selected[i] && !selected[i + 1] {
                    self.slots.swap(i, i + 1);
                    selected.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..PATTERN_SIZE {
                if selected[i] && !selected[i - 1] {
                    self.slots.swap(i, i - 1);
                    selected.swap(i, i - 1);
                }
            }
        }
    }

    /// Moves the bead at `from` to position `to`; the beads in between close up
    /// or make room (like dragging a bead along the pattern).
    pub fn move_slot(&mut self, from: usize, to: usize) {
        move_item(&mut self.slots, from, to);
    }

    /// Moves every bead one place along, the end one wrapping round (blanks are
    /// dropped). Left: the first bead goes to the end.
    pub fn rotate(&mut self, right: bool) {
        let mut b = self.beads();
        if b.len() > 1 {
            if right {
                b.rotate_right(1);
            } else {
                b.rotate_left(1);
            }
        }
        self.clear();
        for (i, bead) in b.into_iter().enumerate() {
            self.slots[i] = Some(bead);
        }
    }

    /// The old "<<Reorder>>" button: drop blanks, then rotate one step right
    /// (the last bead wraps to the front).
    pub fn reorder(&mut self) {
        let mut b = self.beads();
        if b.len() > 1 {
            b.rotate_right(1);
        }
        self.clear();
        for (i, bead) in b.into_iter().enumerate() {
            self.slots[i] = Some(bead);
        }
    }

    fn from_beads(beads: &[usize]) -> Self {
        let mut p = Pattern::default();
        for (i, &b) in beads.iter().take(PATTERN_SIZE).enumerate() {
            p.slots[i] = Some(b);
        }
        p
    }
}

/// Moves `items[from]` to `to`, shifting the ones in between (used for a
/// pattern's slots and for which slots are selected, so both move together).
pub fn move_item<T: Copy>(items: &mut [T], from: usize, to: usize) {
    if from >= items.len() || to >= items.len() || from == to {
        return;
    }
    let v = items[from];
    if from < to {
        items.copy_within(from + 1..=to, from);
    } else {
        items.copy_within(to..from, to + 1);
    }
    items[to] = v;
}

fn pick_distinct(catalog_size: usize, count: usize, rng: &mut Rng) -> Vec<usize> {
    let mut all: Vec<usize> = (0..catalog_size).collect();
    rng.shuffle(&mut all);
    all.truncate(count.min(catalog_size));
    all
}

/// Random pattern: pick min..=max distinct beads, then a random pattern length
/// (at least the number of beads, at most 12) filled with those beads.
pub fn auto_random(catalog_size: usize, min_beads: usize, max_beads: usize, rng: &mut Rng) -> Pattern {
    if catalog_size == 0 {
        return Pattern::default();
    }
    let n = rng.between(min_beads.clamp(1, PATTERN_SIZE), max_beads.clamp(1, PATTERN_SIZE));
    let chosen = pick_distinct(catalog_size, n, rng);
    arrange_random(&chosen, false, rng)
}

/// Lays out beads that were already chosen: a random pattern length (at least
/// one of each, at most 12), shuffled. With `keep_order`, the beads are used once
/// each in the given order (e.g. light to dark).
pub fn arrange_random(chosen: &[usize], keep_order: bool, rng: &mut Rng) -> Pattern {
    let chosen = &chosen[..chosen.len().min(PATTERN_SIZE)];
    if chosen.is_empty() || keep_order {
        return Pattern::from_beads(chosen);
    }
    let length = rng.between(chosen.len(), PATTERN_SIZE);
    let mut seq = chosen.to_vec(); // every chosen bead appears at least once
    while seq.len() < length {
        seq.push(chosen[rng.between(0, chosen.len() - 1)]);
    }
    rng.shuffle(&mut seq);
    Pattern::from_beads(&seq)
}

/// Lays out already-chosen beads symmetrically: 1 2 .. n .. 2 (n at most 7).
pub fn arrange_symmetric(chosen: &[usize]) -> Pattern {
    let chosen = &chosen[..chosen.len().min((PATTERN_SIZE + 2) / 2)];
    let mut seq = chosen.to_vec();
    if chosen.len() > 2 {
        seq.extend(chosen[1..chosen.len() - 1].iter().rev());
    }
    Pattern::from_beads(&seq)
}

/// Symmetrical pattern: n distinct beads (n capped at 7) laid out 1 2 .. n .. 2,
/// so repeats read 1 2 3 4 3 2 | 1 2 3 4 3 2 | ...
pub fn auto_symmetric(catalog_size: usize, min_beads: usize, max_beads: usize, rng: &mut Rng) -> Pattern {
    if catalog_size == 0 {
        return Pattern::default();
    }
    const MAX_SYM: usize = (PATTERN_SIZE + 2) / 2; // 7
    let n = rng.between(min_beads.clamp(1, MAX_SYM), max_beads.clamp(1, MAX_SYM));
    arrange_symmetric(&pick_distinct(catalog_size, n, rng))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_and_reorder_like_the_manual() {
        let mut p = Pattern::from_beads(&[10, 11, 12, 11]);
        let mut sel = [false; PATTERN_SIZE];
        sel[1] = true;
        sel[2] = true;
        sel[3] = true; // select beads 2-4 and shift right
        p.shift_selected(&mut sel, true);
        assert_eq!(&p.slots[..5], &[Some(10), None, Some(11), Some(12), Some(11)]);
        assert!(sel[2] && sel[3] && sel[4] && !sel[1]);

        p.reorder(); // compact + rotate right
        assert_eq!(p.beads(), vec![11, 10, 11, 12]);
    }

    #[test]
    fn dragging_and_rotating() {
        let mut p = Pattern::default();
        for (i, b) in [1, 2, 3, 4, 5].into_iter().enumerate() {
            p.slots[i] = Some(b);
        }
        p.move_slot(0, 3); // drag the first bead to the 4th place
        assert_eq!(p.beads(), vec![2, 3, 4, 1, 5]);
        p.move_slot(4, 1); // and the last back to the 2nd
        assert_eq!(p.beads(), vec![2, 5, 3, 4, 1]);
        p.move_slot(2, 2);
        p.move_slot(0, 99); // out of range: nothing happens
        assert_eq!(p.beads(), vec![2, 5, 3, 4, 1]);
        p.rotate(false);
        assert_eq!(p.beads(), vec![5, 3, 4, 1, 2]);
        p.rotate(true);
        assert_eq!(p.beads(), vec![2, 5, 3, 4, 1]);
        let mut sel = [true, false, false];
        move_item(&mut sel, 0, 2);
        assert_eq!(sel, [false, false, true]);
    }

    #[test]
    fn automatic_patterns() {
        let mut rng = Rng::new(42);
        for _ in 0..500 {
            let r = auto_random(20, 2, 3, &mut rng);
            assert!((2..=PATTERN_SIZE).contains(&r.bead_count()));

            let s = auto_symmetric(50, 4, 4, &mut rng).beads(); // 1 2 3 4 3 2
            assert_eq!(s.len(), 6);
            assert_eq!((s[1], s[2]), (s[5], s[4]));

            assert_eq!(auto_symmetric(50, 9, 9, &mut rng).bead_count(), 12); // capped at 7
        }
        assert!(auto_random(0, 3, 7, &mut rng).is_empty());
    }
}
