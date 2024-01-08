use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SKKDictionary {
    pub okuri_ari: BTreeMap<String, Vec<String>>,
    pub okuri_nasi: BTreeMap<String, Vec<String>>,
}
