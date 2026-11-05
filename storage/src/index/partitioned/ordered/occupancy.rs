//! A two-level bitmap of the partitions that may hold keys, so ordered navigation skips runs of
//! empty partitions in a few word operations instead of probing each partition.
//!
//! The bitmap over-approximates occupancy: a partition's bit is set when a key is inserted into it
//! and is not cleared when the partition later empties, so callers confirm each candidate against
//! the partition itself. Partitions only empty through removals, so at scale almost every set bit
//! is a non-empty partition.

/// Bits per word.
const WORD: usize = u64::BITS as usize;

/// The partitions that may hold keys: `words` has one bit per partition and `summary` one bit per
/// non-zero word of `words`.
#[derive(Debug, Default)]
pub(super) struct Occupancy {
    words: Vec<u64>,
    summary: Vec<u64>,
}

impl Occupancy {
    /// An empty bitmap over `partitions` partitions.
    pub(super) fn new(partitions: usize) -> Self {
        let words = partitions.div_ceil(WORD);
        Self {
            words: vec![0; words],
            summary: vec![0; words.div_ceil(WORD)],
        }
    }

    /// Marks partition `i` as possibly holding keys.
    pub(super) fn set(&mut self, i: usize) {
        let w = i / WORD;
        self.words[w] |= 1 << (i % WORD);
        self.summary[w / WORD] |= 1 << (w % WORD);
    }

    /// The smallest marked partition at or above `i`.
    pub(super) fn next_at_or_after(&self, i: usize) -> Option<usize> {
        let w = i / WORD;
        let word = *self.words.get(w)? & (u64::MAX << (i % WORD));
        if word != 0 {
            return Some(w * WORD + word.trailing_zeros() as usize);
        }
        let w = self.next_word_at_or_after(w + 1)?;
        Some(w * WORD + self.words[w].trailing_zeros() as usize)
    }

    /// The largest marked partition at or below `i`.
    pub(super) fn prev_at_or_before(&self, i: usize) -> Option<usize> {
        let i = i.min((self.words.len() * WORD).checked_sub(1)?);
        let w = i / WORD;
        let word = self.words[w] & low_mask(i % WORD);
        if word != 0 {
            return Some(w * WORD + last_bit(word));
        }
        let w = self.prev_word_at_or_before(w.checked_sub(1)?)?;
        Some(w * WORD + last_bit(self.words[w]))
    }

    /// The smallest non-zero word at or above `w`.
    fn next_word_at_or_after(&self, w: usize) -> Option<usize> {
        let mut s = w / WORD;
        let mut bits = *self.summary.get(s)? & (u64::MAX << (w % WORD));
        while bits == 0 {
            s += 1;
            bits = *self.summary.get(s)?;
        }
        Some(s * WORD + bits.trailing_zeros() as usize)
    }

    /// The largest non-zero word at or below `w`.
    fn prev_word_at_or_before(&self, w: usize) -> Option<usize> {
        let mut s = w / WORD;
        let mut bits = self.summary[s] & low_mask(w % WORD);
        while bits == 0 {
            s = s.checked_sub(1)?;
            bits = self.summary[s];
        }
        Some(s * WORD + last_bit(bits))
    }
}

/// The bits at or below `bit`.
const fn low_mask(bit: usize) -> u64 {
    u64::MAX >> (WORD - 1 - bit)
}

/// The index of the highest set bit of a non-zero `word`.
const fn last_bit(word: u64) -> usize {
    WORD - 1 - word.leading_zeros() as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use commonware_utils::TestRng;
    use rand::RngExt as _;

    #[test]
    fn empty_finds_nothing() {
        let occupancy = Occupancy::new(100);
        assert_eq!(occupancy.next_at_or_after(0), None);
        assert_eq!(occupancy.prev_at_or_before(99), None);
        assert_eq!(occupancy.next_at_or_after(1000), None);
        assert_eq!(Occupancy::new(0).prev_at_or_before(5), None);
    }

    #[test]
    fn matches_a_linear_scan() {
        let mut rng = TestRng::new(7);
        for &(partitions, marks) in &[(1, 1), (64, 3), (65, 2), (4097, 5), (300_000, 40), (1 << 16, 0)] {
            let mut occupancy = Occupancy::new(partitions);
            let mut marked = vec![false; partitions];
            for _ in 0..marks {
                let i = rng.random_range(0..partitions);
                occupancy.set(i);
                marked[i] = true;
            }
            for _ in 0..2000 {
                let i = rng.random_range(0..partitions + 70);
                let next = (i..partitions).find(|&p| marked[p]);
                let prev = (0..=i.min(partitions - 1)).rev().find(|&p| marked[p]);
                assert_eq!(occupancy.next_at_or_after(i), next, "next {i} of {partitions}");
                assert_eq!(occupancy.prev_at_or_before(i), prev, "prev {i} of {partitions}");
            }
        }
    }
}
