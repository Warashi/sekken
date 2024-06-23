use std::collections::{BTreeMap, BTreeSet};

use itertools::{EitherOrBoth::*, Itertools};

#[derive(Debug, Clone)]
pub struct Segmenter {
    chars: BTreeMap<char, SegmentChar>,
}

impl Default for Segmenter {
    fn default() -> Self {
        let small = ('a'..='z').map(|c| (c, SegmentChar::continuous()));
        let large = ('A'..='Z').map(|c| (c, SegmentChar::pre_okuri()));
        let sticky = [';'].map(|c| (c, SegmentChar::sticky()));
        let prefix_postfix = ['>'].map(|c| (c, SegmentChar::prefix_postfix()));

        let chars = small
            .chain(large)
            .chain(sticky)
            .chain(prefix_postfix)
            .collect();

        return Segmenter { chars };
    }
}

impl sekken_lattice::Segmenter for Segmenter {
    fn segment(self: &Self, sentence: &String) -> Vec<sekken_lattice::Node> {
        let mut cur: BTreeSet<(usize, Option<usize>, String)> =
            BTreeSet::from([(0, None, "".to_string())]);

        for (i, e) in sentence
            .chars()
            .zip_longest(sentence.chars().skip(1))
            .enumerate()
        {
            match e {
                Both(c, n) => {
                    let sc = self.chars.get(&c);
                    let sc = *sc.unwrap_or(&SegmentChar::default());

                    let mut tmp = BTreeSet::new();

                    if sc.replace_okuri {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i + 1), s + &n.to_string()),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i + 1, None, "".to_string()));
                    }

                    cur = tmp;
                }
                Left(c) => {
                    let sc = self.chars.get(&c);
                    let sc = *sc.unwrap_or(&SegmentChar::default());

                    let mut tmp = BTreeSet::new();
                    if sc.solo {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i), s),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i, Some(i + 1), c.to_string()));
                        tmp.insert((i + 1, None, "".to_string()));
                    }

                    if sc.pre {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i), s),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i, None, c.to_string()));
                    }

                    if sc.pre_okuri {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i), s + &c.to_string()),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i, None, c.to_string()));
                    }

                    if sc.post {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i + 1), s + &c.to_string()),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i + 1, None, "".to_string()));
                    }

                    if sc.replace {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i + 1), s),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i + 1, None, "".to_string()));
                    }

                    if sc.replace_okuri {
                        let a: BTreeSet<_> = cur
                            .clone()
                            .into_iter()
                            .map(|(b, e, s)| match e {
                                Some(_) => (b, e, s),
                                None => (b, Some(i), s),
                            })
                            .collect();
                        let mut a = a;
                        tmp.append(&mut a);
                        tmp.insert((i, None, "".to_string()));
                    }

                    cur = tmp;
                }
                Right(_) => unreachable!(),
            }
        }

        return cur
            .iter()
            .map(|(b, e, s)| match e {
                Some(e) => sekken_lattice::Node::new(*b, *e, s.to_string()),
                None => sekken_lattice::Node::new(*b, sentence.chars().count(), s.to_string()),
            })
            .collect();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentChar {
    solo: bool,
    pre: bool,
    pre_okuri: bool,
    post: bool,
    replace: bool,
    replace_okuri: bool,
}

impl Default for SegmentChar {
    fn default() -> Self {
        return Self {
            solo: true,
            pre: false,
            pre_okuri: false,
            post: false,
            replace: false,
            replace_okuri: false,
        };
    }
}

impl SegmentChar {
    fn continuous() -> Self {
        return Self {
            solo: false,
            pre: false,
            pre_okuri: false,
            post: false,
            replace: false,
            replace_okuri: false,
        };
    }

    fn pre_okuri() -> Self {
        return Self {
            solo: false,
            pre: true,
            pre_okuri: true,
            post: false,
            replace: false,
            replace_okuri: false,
        };
    }

    fn sticky() -> Self {
        return Self {
            solo: false,
            pre: false,
            pre_okuri: false,
            post: false,
            replace: true,
            replace_okuri: true,
        };
    }

    fn prefix_postfix() -> Self {
        return Self {
            solo: false,
            pre: true,
            pre_okuri: false,
            post: true,
            replace: false,
            replace_okuri: false,
        };
    }
}
