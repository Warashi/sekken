use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Context as _;
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub surface: String,

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

#[derive(Clone, Debug)]
pub struct Lattice {
    sentence: String,
    begin_nodes: Vec<Vec<Rc<RefCell<Node>>>>,
    end_nodes: Vec<Vec<Rc<RefCell<Node>>>>,
    all_nodes: Vec<Rc<RefCell<Node>>>,
}

pub trait SegmentConverter {
    fn segconvert(&self, sentence: &String) -> Vec<Node>;
}

impl<T: SegmentConverter + ?Sized> SegmentConverter for Box<T> {
    #[inline]
    fn segconvert(&self, sentence: &String) -> Vec<Node> {
        (**self).segconvert(sentence)
    }
}

pub trait Converter {
    fn convert(&self, word: &String) -> Vec<String>;
}

pub trait Dict {
    fn get(&self, word: &String) -> Vec<String>;
}

impl<T: Dict + ?Sized> Dict for Box<T> {
    #[inline]
    fn get(&self, sentence: &String) -> Vec<String> {
        (**self).get(sentence)
    }
}

pub trait CostManager {
    fn emission_cost(&self, node: &Node) -> i128;
    fn transition_cost(&self, left: &Node, right: &Node) -> i128;
}

impl<T: CostManager + ?Sized> CostManager for Box<T> {
    #[inline]
    fn emission_cost(&self, node: &Node) -> i128 {
        (**self).emission_cost(node)
    }

    #[inline]
    fn transition_cost(&self, left: &Node, right: &Node) -> i128 {
        (**self).transition_cost(left, right)
    }
}

impl Lattice {
    pub fn new(sentence: &String) -> Result<Self> {
        let sentence = sentence.clone();
        let len = sentence.chars().count();
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
        segconverter: &impl SegmentConverter,
        dict: &impl Dict,
    ) -> Result<()> {
        let segments = segconverter.segconvert(&self.sentence);

        let nodes = segments.iter().filter(|n| n.start != n.end);

        for n in nodes {
            for w in dict.get(&n.surface) {
                self.insert(n.start, n.end, w)?;
            }
        }

        Ok(())
    }

    pub fn viterbi(self: &mut Self, manager: &impl CostManager) -> Result<Vec<Node>> {
        let mut bos = self.end_nodes[0][0]
            .try_borrow_mut()
            .context("try borrow mut bos")?;
        bos.min_cost = Some(0);
        drop(bos);

        for i in 0..=self.sentence.chars().count() {
            for rnode in &self.begin_nodes[i] {
                let mut rnode = rnode.try_borrow_mut().context("try borrow mut rnode")?;
                rnode.min_cost = Some(i128::MAX);

                for lnode in &self.end_nodes[i] {
                    let lnodeb = lnode.clone();
                    let lnodeb = lnodeb.try_borrow().context("try borrow lnodeb")?;

                    let current_cost = lnodeb.min_cost.unwrap();

                    if current_cost == i128::MAX {
                        unreachable!()
                    }

                    let cost = current_cost
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

        fn prev(node: Rc<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {
            let node = node.clone();
            let Ok(node) = node.try_borrow() else {
                return None;
            };
            return node.min_prev.clone();
        }

        let mut push = |node: Rc<RefCell<Node>>| -> Result<()> {
            let node = node.try_borrow().context("try borrow node")?;
            results.push(node.clone());
            return Ok(());
        };

        let eos = self.begin_nodes[self.sentence.chars().count()][0].clone();

        let mut node = eos;
        while let Some(n) = prev(node.clone()) {
            push(n.clone()).context("push result")?;
            node = n;
        }

        results.reverse();

        return Ok(results);
    }
}
