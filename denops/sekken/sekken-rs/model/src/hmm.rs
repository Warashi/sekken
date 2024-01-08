use std::collections::BTreeMap;
use std::io::{Read, Write};

use anyhow::Context as _;
use anyhow::Result;
use capnp::message::{Builder, ReaderOptions};
use capnp::serialize;
use serde::{Deserialize, Serialize};

use crate::hmm_capnp::hidden_markov_model;

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct HiddenMarkovModel {
    n_hidden: usize,
    n_observed: usize,

    pub initial: Vec<f64>,
    pub transition: Vec<Vec<f64>>,
    pub emission: Vec<Vec<f64>>,

    pub id2char: BTreeMap<usize, char>,
    pub char2id: BTreeMap<char, usize>,
}

impl HiddenMarkovModel {
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
            .get_root::<hidden_markov_model::Reader>()
            .context("Failed to get root")?;

        let n_hidden = reader.get_n_hidden() as usize;
        let n_observed = reader.get_n_observed() as usize;

        let initial = reader.get_initial()?.iter().collect();
        let transition = reader
            .get_transition()?
            .iter()
            .collect::<Vec<_>>()
            .chunks(n_hidden)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|x| x.iter().copied().collect::<Vec<_>>())
            .collect();
        let emission = reader
            .get_emission()?
            .iter()
            .collect::<Vec<_>>()
            .chunks(n_hidden)
            .into_iter()
            .map(|x| x.iter().copied().collect::<Vec<_>>())
            .collect();

        let chars = reader.get_chars()?;
        let chars = chars
            .iter()
            .map(|x| -> Result<(usize, char)> {
                Ok((
                    x.get_id() as usize,
                    char::from_u32(x.get_code()).context("failed to convert unicode code point")?,
                ))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to get chars")?;
        let id2char = chars
            .iter()
            .copied()
            .map(|(id, c)| (id, c))
            .collect::<BTreeMap<_, _>>();
        let char2id = chars
            .iter()
            .copied()
            .map(|(id, c)| (c, id))
            .collect::<BTreeMap<_, _>>();

        Ok(HiddenMarkovModel {
            n_hidden,
            n_observed,
            initial,
            transition,
            emission,
            id2char,
            char2id,
        })
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
        let writer = writer.auto_finish();
        let mut msg = Builder::new_default();
        let mut builder = msg.init_root::<hidden_markov_model::Builder>();

        let mut initial = builder.reborrow().init_initial(self.initial.len() as u32);
        for (i, x) in self.initial.iter().enumerate() {
            initial.set(i as u32, *x);
        }

        let mut transition = builder.reborrow().init_transition((self.n_hidden * self.n_hidden) as u32);
        for (i, x) in self.transition.iter().enumerate() {
            for (j, y) in x.iter().enumerate() {
                transition.set(i as u32 * self.n_hidden as u32 + j as u32, *y);
            }
        }

        let mut emission = builder.reborrow().init_emission((self.n_hidden * self.n_observed) as u32);
        for (i, x) in self.emission.iter().enumerate() {
            for (j, y) in x.iter().enumerate() {
                emission.set(i as u32 * self.n_observed as u32 + j as u32, *y);
            }
        }

        let mut chars = builder.reborrow().init_chars(self.id2char.len() as u32);
        for (i, (id, c)) in self.id2char.iter().enumerate() {
            let mut char = chars.reborrow().get(i as u32);
            char.set_id(*id as u64);
            char.set_code(*c as u32);
        }

        serialize::write_message(writer, &msg).context("Failed to write message")?;
        Ok(())
    }
}
