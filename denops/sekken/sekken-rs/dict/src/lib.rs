use std::collections::BTreeMap;
use std::io::Read;

use anyhow::Context as _;
use anyhow::Result;

use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Debug)]
pub struct Dict {
    inner: BTreeMap<String, Vec<String>>,
}

impl sekken_lattice::Dict for Dict {
    fn get(self: &Self, word: &String) -> Vec<String> {
        let mut out = self.inner.get(word).unwrap_or(&Vec::new()).clone();
        // workaround
        // TODO: separate dict for identity, katakana
        // TODO: how to add alphabet?
        out.push(word.clone());
        return out;
    }
}

impl Dict {
    pub fn from_skk_json<R: Read>(r: R) -> Result<Self> {
        #[derive(Serialize, Deserialize, Debug, Clone)]
        struct SKKDictJSON {
            pub okuri_ari: BTreeMap<String, Vec<String>>,
            pub okuri_nasi: BTreeMap<String, Vec<String>>,
        }

        let mut dict: SKKDictJSON = serde_json::from_reader(r).context("parse json dictionary")?;

        let mut out = Dict::default();
        out.inner.append(&mut dict.okuri_ari);

        for (k, v) in out.inner.iter_mut() {
            if let Some(mut n) = dict.okuri_nasi.remove(k) {
                v.append(&mut n)
            }
        }

        out.inner.append(&mut dict.okuri_nasi);

        return Ok(out);
    }
}
