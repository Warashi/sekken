//! SKK 辞書（SKK-JISYO 形式）の読み込みと検索。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result};

/// 見出し語から候補への写像。okuri-ari は `かk` のように末尾に送りの子音を持つ。
#[derive(Debug, Default, Clone)]
pub struct Dictionary {
    okuri_ari: BTreeMap<String, Vec<String>>,
    okuri_nasi: BTreeMap<String, Vec<String>>,
}

impl Dictionary {
    /// SKK-JISYO 形式のテキストを解析する。
    pub fn parse(text: &str) -> Dictionary {
        let mut dict = Dictionary::default();
        let mut okuri_ari = true;
        for line in text.lines() {
            if line.starts_with(";; okuri-nasi") {
                okuri_ari = false;
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
            let map = if okuri_ari {
                &mut dict.okuri_ari
            } else {
                &mut dict.okuri_nasi
            };
            map.entry(key.to_string()).or_default().extend(candidates);
        }
        dict
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
        self.okuri_nasi.get(yomi).map_or(&[], Vec::as_slice)
    }

    /// 送りありの候補。`yomi` は語幹のかな、`okuri` は送り仮名の先頭子音。
    pub fn okuri_ari(&self, yomi: &str, okuri: char) -> &[String] {
        let key = format!("{yomi}{okuri}");
        self.okuri_ari.get(&key).map_or(&[], Vec::as_slice)
    }

    pub fn okuri_nasi_entries(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.okuri_nasi.iter()
    }

    pub fn okuri_ari_entries(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.okuri_ari.iter()
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
