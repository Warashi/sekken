use std::collections::BTreeMap;

#[derive(Default, Debug, Clone)]
pub struct Segmenter {
    chars: BTreeMap<char, SegmentChar>,
}

#[derive(Debug, Clone, Copy)]
pub struct SegmentChar {
    solo: bool,
    pre: bool,
    post: bool,
    replace: bool,
}

impl Default for SegmentChar {
    fn default() -> Self {
        return Self {
            solo: true,
            pre: false,
            post: false,
            replace: false,
        };
    }
}
