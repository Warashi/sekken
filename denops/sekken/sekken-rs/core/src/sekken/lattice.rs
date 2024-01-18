use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Context, Result};

use crate::viterbi::Node;
use sekken_model::compact::CompactModel;

#[derive(Clone, Debug)]
pub struct Entry {
    head: Rc<RefCell<Node<char>>>,
    tail: Rc<RefCell<Node<char>>>,
}

impl Entry {
    pub fn new(
        head: Rc<RefCell<Node<char>>>,
        tail: Rc<RefCell<Node<char>>>,
    ) -> Entry {
        Entry { head, tail,  }
    }
}

#[derive(Clone, Debug)]
pub struct Lattice {
    entries: Vec<Vec<Entry>>,
}

impl Lattice {
    pub fn new(entries: Vec<Vec<Entry>>) -> Lattice {
        Lattice { entries }
    }

    pub fn viterbi(&self, model: &CompactModel, top_n: usize) -> Result<Vec<(u16, String)>> {
        if self.entries.is_empty() {
            return Ok(Vec::new());
        }

        let mut entries = self.entries.clone();
        let last = Node::new('\0', 0);
        entries.push(vec![Entry::new(last.clone(), last.clone())]);

        let mut left = entries.first().context("entries is empty")?;
        for right in entries.iter().skip(1) {
            for left_entry in left {
                for right_entry in right {
                    let score =
                        model.get_bigram_cost(left_entry.tail.borrow().value, right_entry.head.borrow().value);
                    let mut right_node = right_entry.head.borrow_mut();
                    right_node.add_left(left_entry.tail.clone(), score);
                }
            }

            left = right;
        }

        let result = last
            .borrow_mut()
            .calculate(top_n)
            .context("failed to calculate viterbi")?;

        Ok(result
            .iter()
            .map(|((score, _), path)| (*score, path.iter().collect::<String>()))
            .collect())
    }
}
