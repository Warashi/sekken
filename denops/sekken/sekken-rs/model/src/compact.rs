use std::collections::BTreeMap;

#[cfg(feature = "load")]
use std::io::Read;
use capnp::message::ReaderOptions;

#[cfg(feature = "save")]
use std::io::Write;
use capnp::message::Builder;

use anyhow::Context as _;
use anyhow::Result;
use capnp::serialize;
use serde::{Deserialize, Serialize};

use crate::compact_capnp::compact_model;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CompactModel {
    unigram_cost: BTreeMap<u32, u8>,
    bigram_cost: BTreeMap<u64, u8>,
}

impl CompactModel {
    pub fn new() -> Self {
        return Self::default();
    }

    #[cfg(feature = "load")]
    pub fn load<R: Read>(reader: R) -> Result<Self> {
        use ruzstd::streaming_decoder::StreamingDecoder as Decoder;
        let reader = Decoder::new(reader).context("Failed to create zstd decoder")?;
        let reader = serialize::read_message(
            reader,
            ReaderOptions {
                traversal_limit_in_words: None,
                nesting_limit: i32::MAX,
            },
        )
        .context("Failed to read message")?;
        let reader = reader
            .get_root::<compact_model::Reader>()
            .context("Failed to get root")?;

        let unigram_cost = reader.get_unigram().context("Failed to get entries")?;
        let unigram_cost = unigram_cost.iter().map(|entry| {
            let key = entry.get_key();
            let cost = entry.get_value();
            (key, cost)
        });
        let unigram_cost = unigram_cost.collect();

        let bigram_cost = reader.get_bigram().context("Failed to get entries")?;
        let bigram_cost = bigram_cost.iter().map(|entry| {
            let key = entry.get_key();
            let cost = entry.get_value();
            (key, cost)
        });
        let bigram_cost = bigram_cost.collect();

        Ok(CompactModel { unigram_cost, bigram_cost })
    }

    #[cfg(feature = "save")]
    pub fn save<W: Write>(&self, writer: &mut W) -> Result<()> {
        use zstd::Encoder;
        let writer = Encoder::new(
            writer,
            zstd::compression_level_range()
                .max()
                .unwrap_or(zstd::DEFAULT_COMPRESSION_LEVEL),
        )
        .context("Failed to create zstd encoder")?;
        let mut writer = writer.auto_finish();
        let mut msg = Builder::new_default();
        let root = msg.init_root::<compact_model::Builder>();
        let mut unigram_list = root.init_unigram(self.unigram_cost.len() as u32);
        for (i, (key, cost)) in self.unigram_cost.iter().enumerate() {
            let mut entry = unigram_list.reborrow().get(i as u32);
            entry.set_key(*key);
            entry.set_value(*cost);
        }
        serialize::write_message(&mut writer, &msg).context("Failed to write message")?;

        let root = msg.init_root::<compact_model::Builder>();
        let mut bigram_list = root.init_bigram(self.bigram_cost.len() as u32);
        for (i, (key, cost)) in self.bigram_cost.iter().enumerate() {
            let mut entry = bigram_list.reborrow().get(i as u32);
            entry.set_key(*key);
            entry.set_value(*cost);
        }
        serialize::write_message(&mut writer, &msg).context("Failed to write message")?;

        Ok(())
    }

    pub fn get_unigram_cost(&self, c: char) -> u8 {
        *self.unigram_cost.get(&(c as u32)).unwrap_or(&255)
    }

    pub(crate) fn set_unigram_cost(&mut self, key: u32, cost: u8) {
        self.unigram_cost.insert(key, cost);
    }

    pub fn get_bigram_cost(&self, c1: char, c2: char) -> u8 {
        *self
            .bigram_cost
            .get(&((c1 as u64) << 32 | c2 as u64))
            .unwrap_or(&255)
    }

    pub(crate) fn set_bigram_cost(&mut self, key: u64, cost: u8) {
        self.bigram_cost.insert(key, cost);
    }
}
