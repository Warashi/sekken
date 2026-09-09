//! 候補の id 列を trie にまとめ、共有する接頭辞を一度だけ読めるようにする。
//!
//! 節は「そこまでの接頭辞を読み終えた状態」で、次の字を予測する 1 行に対応する。
//! 連鎖への分け方は `session` が決める。ここは列を足して親子を辿れるだけの木。

/// 根は 0 番。
pub struct Trie {
    nodes: Vec<Node>,
}

struct Node {
    id: u32,
    parent: usize,
    children: Vec<usize>,
}

impl Default for Trie {
    fn default() -> Self {
        Trie::new()
    }
}

impl Trie {
    /// 根だけの trie。
    pub fn new() -> Trie {
        Trie {
            nodes: vec![Node {
                id: 0,
                parent: 0,
                children: Vec::new(),
            }],
        }
    }

    /// 列を足し、通る節（根を除く、列と同じ長さ）を返す。既にある節はそのまま使う。
    pub fn insert(&mut self, seq: &[u32]) -> Vec<usize> {
        let mut node = 0;
        seq.iter()
            .map(|&id| {
                node = self.child(node, id);
                node
            })
            .collect()
    }

    fn child(&mut self, node: usize, id: u32) -> usize {
        if let Some(&c) = self.nodes[node]
            .children
            .iter()
            .find(|&&c| self.nodes[c].id == id)
        {
            return c;
        }
        self.nodes.push(Node {
            id,
            parent: node,
            children: Vec::new(),
        });
        let c = self.nodes.len() - 1;
        self.nodes[node].children.push(c);
        c
    }

    pub fn id(&self, node: usize) -> u32 {
        self.nodes[node].id
    }

    /// 親の節。根の親は根。
    pub fn parent(&self, node: usize) -> usize {
        self.nodes[node].parent
    }

    pub fn children(&self, node: usize) -> &[usize] {
        &self.nodes[node].children
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 共有する接頭辞は同じ節になる() {
        let mut trie = Trie::new();
        let paths: Vec<Vec<usize>> = [vec![3, 4, 5], vec![3, 4, 6], vec![3, 4, 5]]
            .iter()
            .map(|s| trie.insert(s))
            .collect();
        assert_eq!(paths[0], paths[2]);
        assert_eq!(paths[0][..2], paths[1][..2]);
        assert_ne!(paths[0][2], paths[1][2]);
        // 根 + 3,4 + 5,6
        assert_eq!(trie.len(), 5);
        assert_eq!(trie.id(paths[1][2]), 6);
    }

    #[test]
    fn 親と子を辿れる() {
        let mut trie = Trie::new();
        let a = trie.insert(&[3, 4, 5]);
        let b = trie.insert(&[3, 6]);
        assert_eq!(trie.parent(a[0]), 0);
        assert_eq!(trie.parent(a[2]), a[1]);
        assert_eq!(trie.children(a[0]), [a[1], b[1]]);
        assert!(trie.children(a[2]).is_empty());
    }

    #[test]
    fn 空の列は節を増やさない() {
        let mut trie = Trie::new();
        assert!(trie.insert(&[]).is_empty());
        assert!(trie.is_empty());
    }
}
