use std::collections::{BTreeMap, BTreeSet};

use itertools::{EitherOrBoth::*, Itertools};

#[derive(Debug, Clone)]
pub struct Segmenter {
    chars: BTreeMap<char, SegmentChar>,
}

#[derive(Debug, Clone)]
pub struct SKK<C: sekken_lattice::Converter> {
    chars: BTreeMap<char, SegmentChar>,
    converter: C,
}

impl<C> Default for SKK<C>
where
    C: sekken_lattice::Converter + Default,
{
    fn default() -> Self {
        let small = ('a'..='z').map(|c| (c, SegmentChar::continuous()));
        let large = ('A'..='Z').map(|c| (c, SegmentChar::pre_okuri())); // TODO: convert to lower
                                                                        // case when segmenting
        let sticky = [';'].map(|c| (c, SegmentChar::sticky()));
        let prefix_postfix = ['>'].map(|c| (c, SegmentChar::prefix_postfix()));

        let chars = small
            .chain(large)
            .chain(sticky)
            .chain(prefix_postfix)
            .collect();

        return Self {
            chars,
            converter: C::default(),
        };
    }
}

impl<C> sekken_lattice::SegmentConverter for SKK<C>
where
    C: sekken_lattice::Converter,
{
    fn segconvert(self: &Self, sentence: &String) -> Vec<sekken_lattice::Node> {
        let mut cur: BTreeSet<(usize, Option<usize>, String)> =
            BTreeSet::from([(0, None, "".to_string())]);

        for (i, e) in sentence
            .chars()
            .zip_longest(sentence.chars().skip(1))
            .enumerate()
        {
            match e {
                // TODO: refactoring
                Both(l, r) => cur = self.process(cur, i, l, Some(r)),
                Left(l) => cur = self.process(cur, i, l, None),
                Right(_) => unreachable!(),
            }
        }

        return cur
            .iter()
            .map(|(b, e, s)| match e {
                Some(e) => vec![sekken_lattice::Node::new(*b, *e, s.to_string())],
                None => self
                    .converter
                    .convert(&s.to_string())
                    .iter()
                    .map(|s| sekken_lattice::Node::new(*b, sentence.chars().count(), s.to_string()))
                    .collect(),
            })
            .flatten()
            .collect();
    }
}

impl<C> SKK<C>
where
    C: sekken_lattice::Converter,
{
    fn process(
        self: &Self,
        current: BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
        l: char,
        r: Option<char>,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        let sc = self.chars.get(&l);
        let sc = *sc.unwrap_or(&SegmentChar::default());

        let mut out = BTreeSet::new();

        if sc.solo {
            out.append(&mut self.solo(&current, i, l));
        }

        if sc.pre {
            out.append(&mut self.pre(&current, i, l));
        }

        if sc.pre_okuri {
            out.append(&mut self.pre_okuri(&current, i, l));
        }

        if sc.post {
            out.append(&mut self.post(&current, i, l));
        }

        if sc.replace {
            out.append(&mut self.replace(&current, i));
        }

        if sc.replace_okuri {
            if let Some(r) = r {
                out.append(&mut self.replace_okuri(&current, i, r));
            }
        }

        out.append(&mut self.continuous(&current, l));

        return out;
    }

    fn solo(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
        l: char,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => vec![(b, e, s)],
                None => self
                    .converter
                    .convert(&s)
                    .iter()
                    .map(|s| (b, Some(i), s.clone()))
                    .collect(),
            })
            .flatten()
            .chain([
                (i, Some(i + 1), l.to_string()),
                (i + 1, None, "".to_string()),
            ])
            .collect();
    }

    fn pre(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
        l: char,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => vec![(b, e, s)],
                None => self
                    .converter
                    .convert(&s)
                    .iter()
                    .map(|s| (b, Some(i), s.clone()))
                    .collect(),
            })
            .flatten()
            .chain([(i, None, l.to_string())])
            .collect();
    }

    fn pre_okuri(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
        l: char,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => vec![(b, e, s)],
                None => self
                    .converter
                    .convert(&s)
                    .iter()
                    .map(|s| (b, Some(i), s.clone() + &l.to_string()))
                    .collect(),
            })
            .flatten()
            .chain([(i, None, l.to_string())])
            .collect();
    }

    fn post(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
        l: char,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => vec![(b, e, s)],
                None => self
                    .converter
                    .convert(&(s + &l.to_string()))
                    .iter()
                    .map(|s| (b, Some(i + 1), s.clone()))
                    .collect(),
            })
            .flatten()
            .chain([(i + 1, None, "".to_string())])
            .collect();
    }

    fn replace(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => vec![(b, e, s)],
                None => self
                    .converter
                    .convert(&s)
                    .iter()
                    .map(|s| (b, Some(i), s.clone()))
                    .collect(),
            })
            .flatten()
            .chain([(i, None, "".to_string())])
            .collect();
    }

    fn replace_okuri(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        i: usize,
        r: char,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => vec![(b, e, s)],
                None => self
                    .converter
                    .convert(&s)
                    .iter()
                    .map(|s| (b, Some(i), s.clone() + &r.to_string()))
                    .collect(),
            })
            .flatten()
            .chain([(i, None, "".to_string())])
            .collect();
    }

    fn continuous(
        self: &Self,
        current: &BTreeSet<(usize, Option<usize>, String)>,
        l: char,
    ) -> BTreeSet<(usize, Option<usize>, String)> {
        return current
            .clone()
            .into_iter()
            .map(|(b, e, s)| match e {
                Some(_) => (b, e, s),
                None => (b, None, s + &l.to_string()),
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
