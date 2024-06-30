use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compact::CompactModel;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NormalModel {
    unigram_score: BTreeMap<u32, f64>,
    bigram_score: BTreeMap<u64, f64>,
}

impl NormalModel {
    pub fn new() -> Self {
        return Self::default();
    }

    pub fn increment_unigram_score(&mut self, c: char) {
        self.unigram_score
            .entry(c as u32)
            .and_modify(|e| *e += 1.0)
            .or_insert(1.0);
    }

    pub fn set_unigram_score(&mut self, c: char, v: f64) {
        self.unigram_score.insert(c as u32, v);
    }

    pub fn increment_bigram_score(&mut self, c1: char, c2: char) {
        self.bigram_score
            .entry((c1 as u64) << 32 | c2 as u64)
            .and_modify(|e| *e += 1.0)
            .or_insert(1.0);
    }

    pub fn set_bigram_score(&mut self, c1: char, c2: char, v: f64) {
        self.bigram_score.insert((c1 as u64) << 32 | c2 as u64, v);
    }

    pub fn compact(&self) -> CompactModel {
        let mut compact = CompactModel::new();

        // TODO: make able to handle 0 < score < inf
        // we are able to handle 1 <= score < inf now
        for (key, score) in self.unigram_score.iter() {
            let score = (1.0 + score).log2() as u8;
            compact.set_unigram_cost(*key, 255 - score);
        }

        for (key, score) in self.bigram_score.iter() {
            let score = (1.0 + score).log2() as u8;
            compact.set_bigram_cost(*key, 255 - score);
        }

        compact
    }
}
