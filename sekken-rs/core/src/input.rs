//! エンジンへの入力。エディタがローマ字をかなにし、区間の種類を決めて送る。
//!
//! エディタは入力中のかな表示をエンジンに頼らず出すので、ローマ字かな変換は
//! どのエディタも自前で持つ。エンジンが元の綴りを受け取り直すのは重複になる
//! ため、入力はかな化済みの区間の列にし、綴りはエンジンに渡さない。

use crate::kana::KanaTable;
use crate::segment::segment;

/// 区間の扱い。エディタが決め、エンジンは文字から推測しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 辞書を引かずかなのまま出す。
    Kana,
    /// 読みとして辞書を引く。
    Convert,
    /// 変換せずそのまま出す（英字など）。
    Literal,
    /// 綴りをそのまま SKK の abbrev の見出しとして引く（`/emacs`）。
    Abbrev,
}

impl Kind {
    /// 辞書を引く区間か。
    pub fn converts(self) -> bool {
        matches!(self, Kind::Convert | Kind::Abbrev)
    }
}

/// 種類とかな（`Literal` なら文字列そのまま）の組。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub kind: Kind,
    pub text: String,
    /// 直後に `>` があり、接頭辞の見出し（`お>`）でも引く。
    pub prefix: bool,
    /// 直前に `>` があり、接尾辞の見出し（`>かい`）でも引く。
    pub suffix: bool,
}

impl Piece {
    pub fn new(kind: Kind, text: impl Into<String>) -> Piece {
        Piece {
            kind,
            text: text.into(),
            prefix: false,
            suffix: false,
        }
    }

    pub fn kana(text: impl Into<String>) -> Piece {
        Piece::new(Kind::Kana, text)
    }

    pub fn convert(text: impl Into<String>) -> Piece {
        Piece::new(Kind::Convert, text)
    }

    pub fn literal(text: impl Into<String>) -> Piece {
        Piece::new(Kind::Literal, text)
    }

    pub fn abbrev(text: impl Into<String>) -> Piece {
        Piece::new(Kind::Abbrev, text)
    }

    /// `>` の印を付ける。
    pub fn with_marks(mut self, prefix: bool, suffix: bool) -> Piece {
        self.prefix = prefix;
        self.suffix = suffix;
        self
    }
}

/// 1 回の変換の入力。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Input {
    pub pieces: Vec<Piece>,
}

impl Input {
    pub fn new(pieces: Vec<Piece>) -> Input {
        Input { pieces }
    }

    /// 大文字・`;` 境界付きのローマ字から組む。コマンドラインと評価のための入口で、
    /// エディタが自前の変換で作って送るものと同じ結果にする。
    /// 語の終わりの `;` `/` `'` `>` は前の区間を閉じる意図しか持たないので、読みの
    /// 無い末尾の区間は送らない。
    /// まだ打っている途中の区間の末尾の子音 1 文字は次の母音を待っているので、
    /// エディタの表示と同じく変換せずに残す（`n` は単独で「ん」になるので除く）。
    /// 閉じた区間はもう待たないので変換する。abbrev と literal の区間は綴りのまま送る。
    pub fn from_roman(table: &KanaTable, roman: &str) -> Input {
        let seg = segment(roman);
        let settled = match seg.segments.split_last() {
            Some((last, rest)) if last.text.is_empty() => rest,
            _ => seg.segments.as_slice(),
        };
        let mut pieces = Vec::new();
        if !seg.head.is_empty() {
            pieces.push(Piece::kana(table.roman2kana(&seg.head)));
        }
        pieces.extend(settled.iter().map(|s| {
            if s.abbrev {
                Piece::abbrev(s.text.clone())
            } else if s.literal {
                Piece::literal(s.text.clone())
            } else {
                Piece::convert(table.roman2kana(&s.text)).with_marks(s.prefix, s.suffix)
            }
        }));
        // 落とした区間を含む最後の区間を見るので、閉じた区間には母音待ちが残らない。
        let source = seg
            .segments
            .last()
            .map_or(seg.head.as_str(), |s| s.text.as_str());
        if let Some(last) = pieces.last_mut()
            && !matches!(last.kind, Kind::Abbrev | Kind::Literal)
            && let Some(c) = source.chars().last()
            && c.is_ascii_lowercase()
            && !"aeioun".contains(c)
        {
            last.text = table.roman2kana(&source[..source.len() - 1]) + &c.to_string();
        }
        Input { pieces }
    }

    /// 変換する区間があるか。無ければ辞書を引かずかなのまま返す入力。
    pub fn has_convert(&self) -> bool {
        self.pieces.iter().any(|p| p.kind.converts())
    }

    /// 全区間の文字列を続けた読み。入力を条件にする採点器に渡す。
    pub fn reading(&self) -> String {
        self.pieces.iter().map(|p| p.text.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_roman(roman: &str) -> Vec<Piece> {
        Input::from_roman(&KanaTable::default_table(), roman).pieces
    }

    #[test]
    fn 大文字境界ごとにかな化した変換区間にする() {
        assert_eq!(
            from_roman("WagahaiHaNekoDearu."),
            [
                Piece::convert("わがはい"),
                Piece::convert("は"),
                Piece::convert("ねこ"),
                Piece::convert("である。"),
            ]
        );
    }

    #[test]
    fn 先頭の小文字はかな区間になる() {
        assert_eq!(
            from_roman("soreHaNekoDa"),
            [
                Piece::kana("それ"),
                Piece::convert("は"),
                Piece::convert("ねこ"),
                Piece::convert("だ"),
            ]
        );
    }

    #[test]
    fn 境界が無ければかな区間だけになる() {
        assert_eq!(from_roman("neko"), [Piece::kana("ねこ")]);
        assert_eq!(from_roman("semi;;koron"), [Piece::kana("せみ;ころん")]);
        assert!(from_roman("").is_empty());
    }

    #[test]
    fn 区間ごとにかな化するので区間をまたいで組にならない() {
        // 続けて変換すると「かに」になる `kan` + `i` も、区間ごとなら「かん」「い」。
        assert_eq!(
            from_roman("KanI"),
            [Piece::convert("かん"), Piece::convert("い")]
        );
    }

    #[test]
    fn 末尾の子音はエディタの表示と同じく変換せずに残す() {
        assert_eq!(
            from_roman("KaK"),
            [Piece::convert("か"), Piece::convert("k")]
        );
        assert_eq!(from_roman("Kak"), [Piece::convert("かk")]);
        assert_eq!(from_roman("nek"), [Piece::kana("ねk")]);
        // n は単独で「ん」になり、母音や記号は待つものが無い。
        assert_eq!(from_roman("Kan"), [Piece::convert("かん")]);
        assert_eq!(from_roman("Neko."), [Piece::convert("ねこ。")]);
    }

    #[test]
    fn 閉じただけの末尾の区間は送らず閉じた区間の子音は変換する() {
        assert_eq!(from_roman("Neko;"), [Piece::convert("ねこ")]);
        assert_eq!(from_roman("Nek;"), [Piece::convert("ねっ")]);
        assert_eq!(from_roman("nek'"), [Piece::kana("ねっ")]);
        assert_eq!(
            from_roman("O>"),
            [Piece::convert("お").with_marks(true, false)]
        );
        assert!(from_roman(";").is_empty());
        assert!(from_roman(">").is_empty());
    }

    /// エディタ側（lisp/sekken-input.el）と同じ区間に分けることを確かめる共通の fixture。
    /// 先読みと確定がずれないよう、送る区間の期待値を両方が読む。
    /// 2 列目の表示はエディタにしかないので読まない。
    #[test]
    fn 共通の_fixture_と一致する() {
        let mut cases = 0;
        for line in include_str!("../input-cases.tsv").lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let roman = fields.next().expect("1 列目はローマ字");
            fields.next().expect("2 列目は表示");
            let expected: Vec<Piece> = fields.map(fixture_piece).collect();
            assert_eq!(from_roman(roman), expected, "{roman}");
            cases += 1;
        }
        assert!(cases > 30, "fixture を読めていない: {cases} 行");
    }

    /// fixture の `KIND:TEXT` を区間にする。`>` の印は KIND に付く。
    fn fixture_piece(field: &str) -> Piece {
        let (kind, text) = field.split_once(':').expect("区間は KIND:TEXT");
        let piece = match kind.split('+').next().expect("KIND がある") {
            "kana" => Piece::kana(text),
            "convert" => Piece::convert(text),
            "literal" => Piece::literal(text),
            "abbrev" => Piece::abbrev(text),
            other => panic!("知らない区間の種類: {other}"),
        };
        piece.with_marks(kind.contains("+prefix"), kind.contains("+suffix"))
    }

    #[test]
    fn スラッシュで開いた区間は綴りのまま_abbrev_区間になる() {
        assert_eq!(from_roman("/emacs"), [Piece::abbrev("emacs")]);
        // 末尾の子音を待つ扱いはしない。
        assert_eq!(
            from_roman("Kyou/GPL;ha"),
            [
                Piece::convert("きょう"),
                Piece::abbrev("GPL"),
                Piece::convert("は")
            ]
        );
        assert!(Input::new(vec![Piece::abbrev("emacs")]).has_convert());
    }

    #[test]
    fn アポストロフィで開いた区間は綴りのまま_literal_区間になる() {
        // 末尾の子音を待つ扱いはしない。
        assert_eq!(from_roman("'emacs"), [Piece::literal("emacs")]);
        assert_eq!(
            from_roman("Kyouha'Emacs;wo"),
            [
                Piece::convert("きょうは"),
                Piece::literal("Emacs"),
                Piece::convert("を")
            ]
        );
        assert!(!Input::new(vec![Piece::literal("emacs")]).has_convert());
    }

    #[test]
    fn 山括弧の印を変換区間に付ける() {
        assert_eq!(
            from_roman("O>Kai"),
            [
                Piece::convert("お").with_marks(true, false),
                Piece::convert("かい").with_marks(false, true),
            ]
        );
    }

    #[test]
    fn 変換区間の有無と読みを返す() {
        let input = Input::new(vec![
            Piece::kana("きょう"),
            Piece::convert("は"),
            Piece::literal("Emacs"),
        ]);
        assert!(input.has_convert());
        assert_eq!(input.reading(), "きょうはEmacs");
        assert!(!Input::new(vec![Piece::kana("ねこ")]).has_convert());
    }
}
