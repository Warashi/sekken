#![allow(dead_code)]

use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
struct Node {
    start: usize,
    end: usize,
    surface: String,

    min_cost: Option<i128>,
    min_prev: Option<Rc<RefCell<Node>>>,
}

impl Node {
    pub fn new(start: usize, end: usize, surface: String) -> Node {
        return Node {
            start,
            end,
            surface,
            min_cost: None,
            min_prev: None,
        };
    }
}

struct Lattice {
    sentence: String,
    begin_nodes: Vec<Vec<Rc<RefCell<Node>>>>,
    end_nodes: Vec<Vec<Rc<RefCell<Node>>>>,
    all_nodes: Vec<Rc<RefCell<Node>>>,
}

trait Segmenter {
    fn segment(&self, sentence: &String) -> Vec<Node>;
}

trait Converter {
    fn convert(&self, word: &String) -> String;
}

trait Dict {
    fn get(&self, word: &String) -> Vec<String>;
}

trait CostManager {
    fn emission_cost(&self, node: &Node) -> i128;
    fn transition_cost(&self, left: &Node, right: &Node) -> i128;
}

impl Lattice {
    pub fn new(sentence: String) -> Result<Self> {
        let len = sentence.len();
        let begin_nodes = vec![Vec::new(); len + 1];
        let end_nodes = vec![Vec::new(); len + 1];
        let all_nodes = Vec::new();

        let mut lattice = Lattice {
            sentence,
            begin_nodes,
            end_nodes,
            all_nodes,
        };

        let bos = lattice.new_node(0, 0, "".to_string());
        lattice.end_nodes[0].push(bos);

        let eos = lattice.new_node(len, len, "".to_string());
        lattice.begin_nodes[len].push(eos);

        return Ok(lattice);
    }

    fn new_node(self: &mut Self, start: usize, end: usize, surface: String) -> Rc<RefCell<Node>> {
        let node = Rc::new(RefCell::new(Node::new(start, end, surface)));

        self.all_nodes.push(node.clone());

        return node;
    }

    fn insert(self: &mut Self, start: usize, end: usize, surface: String) -> Result<()> {
        let node = self.new_node(start, end, surface);

        self.begin_nodes[start].push(node.clone());
        self.end_nodes[end].push(node.clone());

        return Ok(());
    }

    pub fn build(
        self: &mut Self,
        segmenter: impl Segmenter,
        converter: impl Converter,
        dict: impl Dict,
    ) -> Result<()> {
        let segments = segmenter.segment(&self.sentence);

        let nodes = segments.iter().map(|n| {
            return Node::new(n.start, n.end, converter.convert(&n.surface));
        });

        for n in nodes {
            for w in dict.get(&n.surface) {
                self.insert(n.start, n.end, w)?;
            }
        }

        Ok(())
    }

    pub fn viterbi(self: &mut Self, manager: impl CostManager) -> Result<Vec<Node>> {
        let mut bos = self.end_nodes[0][0].try_borrow_mut()?;
        bos.min_cost = Some(0);

        for i in 0..=self.sentence.len() {
            for rnode in &self.begin_nodes[i] {
                let mut rnode = rnode.try_borrow_mut()?;
                rnode.min_cost = Some(i128::MAX);

                for lnode in &self.end_nodes[i] {
                    let lnodeb = lnode.clone();
                    let lnodeb = lnodeb.try_borrow()?;

                    let cost = lnodeb.min_cost.unwrap()
                        + manager.transition_cost(&lnodeb, &rnode)
                        + manager.emission_cost(&rnode);

                    if cost < rnode.min_cost.unwrap() {
                        rnode.min_cost = Some(cost);
                        rnode.min_prev = Some(lnode.clone());
                    }
                }
            }
        }

        let mut results = Vec::new();

        fn next(node: Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
            let node = node.clone();
            let Ok(node) = node.try_borrow() else {
                return None;
            };
            return node.min_prev.clone();
        }

        let mut push = |node: Rc<RefCell<Node>>| -> Result<()> {
            let node = node.try_borrow()?;
            results.push(node.clone());
            return Ok(());
        };

        let mut node = self.begin_nodes[self.sentence.len()][0].clone();
        while let Some(n) = next(node.clone()) {
            push(n.clone())?;
            node = n;
        }

        results.reverse();

        return Ok(results);
    }
}
