//! SKK 辞書（SKK-JISYO 形式）の読み込みと検索。

use std::path::Path;

use anyhow::{Context as _, Result};

/// 見出し語から候補への写像。okuri-ari は `かk` のように末尾に送りの子音を持つ。
#[derive(Debug, Default, Clone)]
pub struct Dictionary {
    okuri_ari: Index,
    okuri_nasi: Index,
}

#[derive(Debug, Default, Clone)]
struct Index {
    entries: Vec<(String, Vec<String>)>,
}

impl Index {
    fn from_entries(mut entries: Vec<(String, Vec<String>)>) -> Index {
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        entries.dedup_by(|next, current| {
            if next.0 == current.0 {
                current.1.append(&mut next.1);
                true
            } else {
                false
            }
        });
        Index { entries }
    }

    fn exact_match(&self, key: &str) -> &[String] {
        self.entries
            .binary_search_by(|(entry, _)| entry.as_str().cmp(key))
            .map_or(&[], |index| self.entries[index].1.as_slice())
    }
}

impl Dictionary {
    /// SKK-JISYO 形式のテキストを解析する。
    pub fn parse(text: &str) -> Dictionary {
        let mut okuri_ari_entries = Vec::new();
        let mut okuri_nasi_entries = Vec::new();
        let mut in_okuri_ari = true;
        for line in text.lines() {
            if line.starts_with(";; okuri-nasi") {
                in_okuri_ari = false;
                continue;
            }
            if line.starts_with(';') || line.is_empty() {
                continue;
            }
            let Some((key, body)) = line.split_once(' ') else {
                continue;
            };
            let candidates = parse_candidates(body);
            if candidates.is_empty() {
                continue;
            }
            let entries = if in_okuri_ari {
                &mut okuri_ari_entries
            } else {
                &mut okuri_nasi_entries
            };
            entries.push((key.to_string(), candidates));
        }
        Dictionary {
            okuri_ari: Index::from_entries(okuri_ari_entries),
            okuri_nasi: Index::from_entries(okuri_nasi_entries),
        }
    }

    /// EUC-JP または UTF-8 の辞書ファイルを読む。
    pub fn load(path: &Path) -> Result<Dictionary> {
        let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
        let text = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => encoding_rs::EUC_JP.decode(&bytes).0.into_owned(),
        };
        Ok(Self::parse(&text))
    }

    /// 送りなしの候補。
    pub fn okuri_nasi(&self, yomi: &str) -> &[String] {
        self.okuri_nasi.exact_match(yomi)
    }

    /// 送りありの候補。`yomi` は語幹のかな、`okuri` は送り仮名の先頭子音。
    pub fn okuri_ari(&self, yomi: &str, okuri: char) -> &[String] {
        let key = format!("{yomi}{okuri}");
        self.okuri_ari.exact_match(&key)
    }

    pub fn okuri_nasi_entries(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.okuri_nasi
            .entries
            .iter()
            .map(|(key, candidates)| (key, candidates))
    }

    pub fn okuri_ari_entries(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.okuri_ari
            .entries
            .iter()
            .map(|(key, candidates)| (key, candidates))
    }
}

/// `/候補;注釈/[き/候補/]/(concat ...)/` から候補の表層形だけを取り出す。
fn parse_candidates(body: &str) -> Vec<String> {
    body.trim()
        .split('/')
        .filter(|s| !s.is_empty())
        .filter(|s| !s.starts_with('[') && !s.starts_with("(concat"))
        .map(|s| s.split_once(';').map_or(s, |(w, _)| w))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
;; -*- mode: fundamental; coding: euc-jp -*-
;; okuri-ari entries.
かk /書/掛;掛ける/
ゆるb /弛;[文語]/緩/
;; okuri-nasi entries.
にほんご /日本語/
わがはい /我輩/吾輩/
GPL /GNU General Public License;(concat \"http:\\057\\057www.gnu.org\")/(concat \"x\")/
";

    #[test]
    fn 送りなしの候補を引く() {
        let d = Dictionary::parse(FIXTURE);
        assert_eq!(d.okuri_nasi("にほんご"), ["日本語"]);
        assert_eq!(d.okuri_nasi("わがはい"), ["我輩", "吾輩"]);
        assert!(d.okuri_nasi("ない").is_empty());
    }

    #[test]
    fn 重複する見出しの候補を辞書順にまとめる() {
        let d = Dictionary::parse(
            "\
;; okuri-nasi entries.
ねこ /猫/
いぬ /犬/
ねこ /ネコ/
",
        );
        assert_eq!(d.okuri_nasi("ねこ"), ["猫", "ネコ"]);
    }

    #[test]
    fn 送りありの候補を語幹と送りの子音で引く() {
        let d = Dictionary::parse(FIXTURE);
        assert_eq!(d.okuri_ari("か", 'k'), ["書", "掛"]);
        assert!(d.okuri_ari("か", 'r').is_empty());
    }

    #[test]
    fn 注釈を落とし角括弧の中を含まない() {
        let d = Dictionary::parse(FIXTURE);
        assert_eq!(d.okuri_ari("ゆる", 'b'), ["弛", "緩"]);
    }

    #[test]
    fn concat_の候補は無視する() {
        let d = Dictionary::parse(FIXTURE);
        assert_eq!(d.okuri_nasi("GPL"), ["GNU General Public License"]);
    }

    #[test]
    fn euc_jp_のファイルを読む() {
        let (bytes, _, _) = encoding_rs::EUC_JP.encode(FIXTURE);
        let dir = std::env::temp_dir().join("sekken-dict-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("jisyo");
        std::fs::write(&path, bytes).unwrap();
        let d = Dictionary::load(&path).unwrap();
        assert_eq!(d.okuri_nasi("にほんご"), ["日本語"]);
    }
}
