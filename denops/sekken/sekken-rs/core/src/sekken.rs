use std::cell::RefCell;

use anyhow::{Context, Result};

use sekken_lattice::{Lattice, SegmentConverter, Dict, CostManager};

pub struct Sekken {
    segconverter: RefCell<Option<Box<dyn SegmentConverter>>>,
    dictionary: RefCell<Option<Box<dyn Dict>>>,
    model: RefCell<Option<Box<dyn CostManager>>>,
}

impl Default for Sekken {
    fn default() -> Self {
        Self::new()
    }
}

impl Sekken {
    pub fn new() -> Sekken {
        Sekken {
            segconverter: RefCell::new(None),
            dictionary: RefCell::new(None),
            model: RefCell::new(None),
        }
    }

    pub fn replace_segconverter(&self, sc: Box<dyn SegmentConverter>) {
        self.segconverter.replace(Some(sc));
    }

    pub fn replace_dictionary(&self, dictionary: Box<dyn Dict>) {
        self.dictionary.replace(Some(dictionary));
    }

    pub fn replace_model(&self, model: Box<dyn CostManager>) {
        self.model.replace(Some(model));
    }

    pub fn henkan(&self, roman: &String, _top_n: usize) -> Result<Vec<String>> {
        let segconverter = self
            .segconverter
            .try_borrow()
            .context("try borrow segconverter")?;
        let segconverter = segconverter.as_ref().context("segconverter is not set")?;

        let dict = self
            .dictionary
            .try_borrow()
            .context("try borrow segconverter")?;
        let dict = dict.as_ref().context("dict is not set")?;

        let model = self.model.try_borrow().context("try borrow segconverter")?;
        let model = model.as_ref().context("model is not set")?;

        let mut lattice = Lattice::new(roman).context("failed to build lattice")?;
        lattice
            .build(segconverter, dict)
            .context("failed to bulid lattice")?;

        let result = lattice
            .viterbi(model)
            .context("failed to caluclate viterbi path")?;

        let result = result
            .iter()
            .map(|n| n.surface.clone())
            .collect::<Vec<_>>()
            .concat();

        return Ok(vec![result]);
    }
}
