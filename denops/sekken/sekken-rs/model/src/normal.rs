use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::compact::CompactModel;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NormalModel {
    unigram_cost: BTreeMap<u32, u128>,
    bigram_cost: BTreeMap<u64, u128>,
}

impl NormalModel {
    pub fn new() -> Self {
        return Self::default();
    }

    pub fn increment_unigram_cost(&mut self, c: char) {
        self.unigram_cost
            .entry(c as u32)
            .and_modify(|e| *e += 1)
            .or_insert(1);
    }

    pub fn increment_bigram_cost(&mut self, c1: char, c2: char) {
        self.bigram_cost
            .entry((c1 as u64) << 32 | c2 as u64)
            .and_modify(|e| *e += 1)
            .or_insert(1);
    }

    pub fn compact(&self) -> CompactModel {
        let mut compact = CompactModel::new();

        for (key, cost) in self.bigram_cost.iter() {
            let cost = (1 + cost).ilog2() as u8;
            compact.set_bigram_cost(*key, 255 - cost);
        }

        compact
    }
}
