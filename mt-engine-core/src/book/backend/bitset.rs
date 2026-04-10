#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct L3Bitset {
    l1: Vec<u64>,
    l2: Vec<u64>,
    l3: Vec<u64>,
}

impl L3Bitset {
    pub fn new(depth: usize) -> Self {
        let l3_words = depth.div_ceil(64);
        let l2_words = l3_words.div_ceil(64);
        let l1_words = l2_words.div_ceil(64);
        Self {
            l1: vec![0; l1_words],
            l2: vec![0; l2_words],
            l3: vec![0; l3_words],
        }
    }

    pub fn set(&mut self, idx: usize) {
        let l3_idx = idx / 64;
        let l3_bit = 1u64 << (idx % 64);
        self.l3[l3_idx] |= l3_bit;
        self.l2[l3_idx / 64] |= 1u64 << (l3_idx % 64);
        self.l1[l3_idx / 4096] |= 1u64 << ((l3_idx / 64) % 64);
    }

    pub fn unset(&mut self, idx: usize) {
        let l3_idx = idx / 64;
        let l3_bit = 1u64 << (idx % 64);
        self.l3[l3_idx] &= !l3_bit;

        if self.l3[l3_idx] == 0 {
            self.l2[l3_idx / 64] &= !(1u64 << (l3_idx % 64));
            if self.l2[l3_idx / 64] == 0 {
                self.l1[l3_idx / 4096] &= !(1u64 << ((l3_idx / 64) % 64));
            }
        }
    }

    pub fn find_first(&self, max: usize) -> Option<usize> {
        for (l1_idx, &l1_word) in self.l1.iter().enumerate() {
            if l1_word == 0 {
                continue;
            }
            let l2_word_idx = (l1_word.trailing_zeros()) as usize + l1_idx * 64;
            let l2_word = self.l2.get(l2_word_idx)?;
            if *l2_word == 0 {
                continue;
            }
            let l3_word_idx = (l2_word.trailing_zeros()) as usize + l2_word_idx * 64;
            let l3_word = self.l3.get(l3_word_idx)?;
            let bit = l3_word.trailing_zeros() as usize;
            let global_idx = l3_word_idx * 64 + bit;
            if global_idx < max {
                return Some(global_idx);
            }
        }
        None
    }

    pub fn find_last(&self, max: usize) -> Option<usize> {
        for l1_idx in (0..self.l1.len()).rev() {
            let l1_word = self.l1[l1_idx];
            if l1_word == 0 {
                continue;
            }
            let l2_idx = (63 - l1_word.leading_zeros()) as usize + l1_idx * 64;
            let l2_word = self.l2.get(l2_idx)?;
            if *l2_word == 0 {
                continue;
            }
            let l3_idx = (63 - l2_word.leading_zeros()) as usize + l2_idx * 64;
            let l3_word = self.l3.get(l3_idx)?;
            let bit = 63 - l3_word.leading_zeros();
            let global_idx = l3_idx * 64 + bit as usize;
            if global_idx < max {
                return Some(global_idx);
            }
        }
        None
    }

    #[inline(always)]
    pub fn test(&self, idx: usize) -> bool {
        let l3_idx = idx / 64;
        let l3_bit = 1u64 << (idx % 64);
        (self.l3[l3_idx] & l3_bit) != 0
    }

    #[inline(always)]
    pub fn clear(&mut self) {
        self.l1.fill(0);
        self.l2.fill(0);
        self.l3.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitset_set_and_find() {
        let mut bs = L3Bitset::new(10000);
        bs.set(4095);
        bs.set(4096);
        assert_eq!(bs.find_first(10000), Some(4095));
        assert_eq!(bs.find_last(10000), Some(4096));
    }
}
