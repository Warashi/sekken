use std::collections::BTreeMap;

#[derive(Default, Clone, Debug)]
pub struct Dict {
    inner: BTreeMap<String, Vec<String>>,
}

impl sekken_lattice::Dict for Dict {
    fn get(self: &Self, word: &String) -> Vec<String> {
        return self.inner.get(word).unwrap_or(&Vec::new()).to_vec();
    }
}
