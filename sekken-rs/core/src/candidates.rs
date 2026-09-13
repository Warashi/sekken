//! 区間ごとの変換候補の生成。

use crate::dictionary::Dictionary;
use crate::input::{Kind, Piece};
use crate::kana::hira2kata;

/// 1 つの区間位置から始まる変換候補。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// 変換後の表層形。
    pub surface: String,
    /// 消費する区間数。送りありで次の区間を使うときは 2。
    pub span: usize,
    /// 辞書での候補の順位（0 始まり）。かな候補は 0。
    pub rank: usize,
    /// 辞書を引かず読みをそのままかなにした候補か。
    pub kana: bool,
}

impl Candidate {
    fn kana(surface: impl Into<String>) -> Candidate {
        Candidate {
            surface: surface.into(),
            span: 1,
            rank: 0,
            kana: true,
        }
    }

    fn ranked(surface: impl Into<String>, span: usize, rank: usize) -> Candidate {
        Candidate {
            surface: surface.into(),
            span,
            rank,
            kana: false,
        }
    }
}

fn is_hiragana(c: char) -> bool {
    matches!(c as u32, 0x3041..=0x3096)
}

/// 読みの途中に置けない字（きゃ の ゃ）。
fn is_small_kana(c: char) -> bool {
    matches!(
        c,
        'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ'
    )
}

/// 区間を「読みになるかな」と「末尾の記号」に分ける。
/// 途中の記号（ターゲット の ー）は読みの一部なので末尾だけを見る。
/// 打ち終えていない綴りの英字（か + k）は読みの側に残す。
fn split_trailing_symbols(text: &str) -> (&str, &str) {
    let reading = text.trim_end_matches(|c: char| !is_hiragana(c) && !c.is_ascii_lowercase());
    text.split_at(reading.len())
}

/// 送り仮名から、SKK の送りあり見出しに付ける子音を取り出す。
///
/// 綴りは受け取らないので、かな 1 字から見出しに使われる字を引く。同じかなでも
/// 綴りによって見出しの字が違う（ち → t と c、じ → z と j）ものは両方を返す。
/// っ で始まる送り仮名は、見出しでは次の字で引く（った → t）ので っ を飛ばす。
/// 打ち終えていない綴りの英字（か + k）はその字を使う。
fn okuri_letters(okuri: &str) -> Vec<char> {
    let mut chars = okuri.chars();
    let mut head = chars.next();
    if head == Some('っ') {
        head = chars.next();
    }
    let Some(c) = head else {
        return Vec::new();
    };
    if c.is_ascii_lowercase() {
        return vec![c];
    }
    let letters: &[char] = match c {
        'あ' => &['a'],
        'い' => &['i'],
        'う' => &['u'],
        'え' => &['e'],
        'お' => &['o'],
        'か' | 'き' | 'く' | 'け' | 'こ' => &['k'],
        'が' | 'ぎ' | 'ぐ' | 'げ' | 'ご' => &['g'],
        'さ' | 'し' | 'す' | 'せ' | 'そ' => &['s'],
        'じ' => &['z', 'j'],
        'ざ' | 'ず' | 'ぜ' | 'ぞ' => &['z'],
        'ち' => &['t', 'c'],
        'た' | 'つ' | 'て' | 'と' => &['t'],
        'だ' | 'ぢ' | 'づ' | 'で' | 'ど' => &['d'],
        'な' | 'に' | 'ぬ' | 'ね' | 'の' => &['n'],
        'は' | 'ひ' | 'ふ' | 'へ' | 'ほ' => &['h'],
        'ば' | 'び' | 'ぶ' | 'べ' | 'ぼ' => &['b'],
        'ぱ' | 'ぴ' | 'ぷ' | 'ぺ' | 'ぽ' => &['p'],
        'ま' | 'み' | 'む' | 'め' | 'も' => &['m'],
        'や' | 'ゆ' | 'よ' => &['y'],
        'ら' | 'り' | 'る' | 'れ' | 'ろ' => &['r'],
        'わ' | 'を' => &['w'],
        'ん' => &['n'],
        _ => &[],
    };
    letters.to_vec()
}

/// 送りあり候補を、子音の字ごとの見出しから順に集める。重複は先の字のものを残す。
fn okuri_ari(dict: &Dictionary, yomi: &str, okuri: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for letter in okuri_letters(okuri) {
        for surface in dict.okuri_ari(yomi, letter) {
            if !out.contains(surface) {
                out.push(surface.clone());
            }
        }
    }
    out
}

/// 位置 `index` の区間から始まる候補をすべて列挙する。
///
/// 変換区間 `わがはいは` は「辞書の読み `わがはい` + 残りのかな `は`」のように
/// 読みの前方一致で分けて引く。送り仮名は残りの先頭（`かく` の `く`）でも、
/// 次の変換区間（`か` `く`）でも表せる。`>` の印が付いた区間は接頭辞（`お>`）
/// や接尾辞（`>かい`）の見出しでも引く。abbrev 区間は綴りをそのまま見出しにする。
/// かな区間とそのまま出す区間は、その文字列 1 つだけを候補にする。
pub fn candidates_at(dict: &Dictionary, pieces: &[Piece], index: usize) -> Vec<Candidate> {
    let piece = &pieces[index];
    match piece.kind {
        Kind::Convert => {}
        Kind::Abbrev => return abbrev(dict, &piece.text),
        Kind::Kana | Kind::Literal => return vec![Candidate::ranked(piece.text.clone(), 1, 0)],
    }
    let (reading, symbols) = split_trailing_symbols(&piece.text);
    let mut out = Vec::new();

    // `>` で区切った次の区間は送り仮名でなく接尾辞。
    if symbols.is_empty()
        && !piece.prefix
        && let Some(next) = pieces.get(index + 1)
        && next.kind == Kind::Convert
    {
        let (okuri, okuri_symbols) = split_trailing_symbols(&next.text);
        for (rank, surface) in okuri_ari(dict, reading, okuri).into_iter().enumerate() {
            out.push(Candidate::ranked(
                format!("{surface}{okuri}{okuri_symbols}"),
                2,
                rank,
            ));
        }
    }

    let chars: Vec<char> = reading.chars().collect();
    for split in (1..=chars.len()).rev() {
        let yomi: String = chars[..split].iter().collect();
        let rest: String = chars[split..].iter().collect();
        // 打ち終えていない綴りは読みにならず、小書きの字の手前は読みの切れ目でない。
        if yomi.chars().any(|c| c.is_ascii()) || rest.starts_with(is_small_kana) {
            continue;
        }
        let suffix = format!("{rest}{symbols}");
        for (rank, surface) in okuri_ari(dict, &yomi, &rest).into_iter().enumerate() {
            out.push(Candidate::ranked(format!("{surface}{suffix}"), 1, rank));
        }
        for (rank, surface) in dict.okuri_nasi(&yomi).iter().enumerate() {
            out.push(Candidate::ranked(format!("{surface}{suffix}"), 1, rank));
        }
        if piece.suffix {
            for (rank, surface) in dict.okuri_nasi(&format!(">{yomi}")).iter().enumerate() {
                out.push(Candidate::ranked(format!("{surface}{suffix}"), 1, rank));
            }
        }
        // 接頭辞は読み全体が見出しで、後ろに残りのかなを続けない。
        if piece.prefix && rest.is_empty() {
            for (rank, surface) in dict.okuri_nasi(&format!("{yomi}>")).iter().enumerate() {
                out.push(Candidate::ranked(format!("{surface}{suffix}"), 1, rank));
            }
        }
        // カタカナ語に助詞が続く `すこっとらんどは` のような入力のための候補。
        if !rest.is_empty() && split >= 2 {
            out.push(Candidate::kana(format!("{}{suffix}", hira2kata(&yomi))));
        }
    }

    out.push(Candidate::kana(piece.text.clone()));
    out.push(Candidate::kana(hira2kata(&piece.text)));
    out
}

/// abbrev 区間の候補。綴りそのままの見出しを引き、無ければ綴りのまま出す。
fn abbrev(dict: &Dictionary, text: &str) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = dict
        .okuri_nasi(text)
        .iter()
        .enumerate()
        .map(|(rank, surface)| Candidate::ranked(surface.clone(), 1, rank))
        .collect();
    out.push(Candidate::kana(text));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = "\
;; okuri-ari entries.
かk /書/掛/
とt /採/取/
とc /採/
おおi /多/
;; okuri-nasi entries.
か /可/
ねこ /猫/
お /尾/
かい /貝/
お> /御/
>かい /会/
emacs /Ｅｍａｃｓ/イーマックス/
";

    fn dict() -> Dictionary {
        Dictionary::parse(FIXTURE)
    }

    fn convert(texts: &[&str]) -> Vec<Piece> {
        texts.iter().map(|t| Piece::convert(*t)).collect()
    }

    fn surfaces(c: &[Candidate]) -> Vec<&str> {
        c.iter().map(|c| c.surface.as_str()).collect()
    }

    #[test]
    fn 送りなし候補とかな候補を出す() {
        let c = candidates_at(&dict(), &convert(&["ねこ"]), 0);
        assert_eq!(surfaces(&c), ["猫", "ねこ", "ネコ"]);
    }

    #[test]
    fn 次の区間を送り仮名として送りあり候補を出す() {
        let c = candidates_at(&dict(), &convert(&["か", "く"]), 0);
        assert_eq!(c[0], Candidate::ranked("書く", 2, 0));
        assert_eq!(c[1], Candidate::ranked("掛く", 2, 1));
        assert_eq!(c[2], Candidate::ranked("可", 1, 0));
    }

    #[test]
    fn 区間内の残りを送り仮名として送りあり候補を出す() {
        let c = candidates_at(&dict(), &convert(&["かきます"]), 0);
        assert!(c.contains(&Candidate::ranked("書きます", 1, 0)));
        assert!(c.contains(&Candidate::ranked("可きます", 1, 0)));
    }

    #[test]
    fn っで始まる送り仮名はっの次の字で送りあり候補を引く() {
        let c = candidates_at(&dict(), &convert(&["とった"]), 0);
        assert!(c.contains(&Candidate::ranked("採った", 1, 0)));
        assert!(c.contains(&Candidate::ranked("取った", 1, 1)));
    }

    #[test]
    fn 次の区間がっで始まっても送りあり候補を引く() {
        let c = candidates_at(&dict(), &convert(&["と", "った"]), 0);
        assert_eq!(c[0], Candidate::ranked("採った", 2, 0));
    }

    #[test]
    fn 綴りが違っても同じかなの送り仮名は両方の見出しを引く() {
        // ち は ti の t と chi の c の両方で登録されている。重複は 1 つにする。
        let c = candidates_at(&dict(), &convert(&["とち"]), 0);
        let okuri: Vec<&Candidate> = c.iter().filter(|c| !c.kana).collect();
        assert_eq!(
            okuri.iter().map(|c| c.surface.as_str()).collect::<Vec<_>>(),
            ["採ち", "取ち"]
        );
    }

    #[test]
    fn 母音で始まる送り仮名も引く() {
        let c = candidates_at(&dict(), &convert(&["おおい"]), 0);
        assert!(c.contains(&Candidate::ranked("多い", 1, 0)));
    }

    #[test]
    fn 打ち終えていない綴りの英字は送り仮名の子音として使う() {
        // `KaK` と打っている途中。エディタは末尾の子音を変換せずに送る。
        let c = candidates_at(&dict(), &convert(&["か", "k"]), 0);
        assert_eq!(c[0], Candidate::ranked("書k", 2, 0));
        let c = candidates_at(&dict(), &convert(&["かk"]), 0);
        assert!(c.contains(&Candidate::ranked("書k", 1, 0)));
    }

    #[test]
    fn 読みの前方一致で辞書を引き残りをかなにする() {
        let c = candidates_at(&dict(), &convert(&["ねこは"]), 0);
        assert_eq!(surfaces(&c), ["猫は", "ネコは", "ねこは", "ネコハ"]);
    }

    #[test]
    fn 小書きの字の手前では読みを切らない() {
        let c = candidates_at(&dict(), &convert(&["きゃ"]), 0);
        assert_eq!(surfaces(&c), ["きゃ", "キャ"]);
    }

    #[test]
    fn カタカナの前方一致にかなを続けた候補を出す() {
        let c = candidates_at(&dict(), &convert(&["すこっとらんどは"]), 0);
        assert!(c.contains(&Candidate::kana("スコットランドは")));
    }

    #[test]
    fn 記号を含む区間はまとめてかなにする() {
        let c = candidates_at(&dict(), &convert(&["ねこ・"]), 0);
        assert!(c.contains(&Candidate::ranked("猫・", 1, 0)));
        assert!(c.contains(&Candidate::kana("ねこ・")));
    }

    #[test]
    fn 途中の長音記号は読みの一部として扱う() {
        let c = candidates_at(&dict(), &convert(&["たーげっとにする"]), 0);
        assert!(c.contains(&Candidate::kana("ターゲットにする")));
    }

    #[test]
    fn 末尾の記号は読みから外して候補に付け直す() {
        let c = candidates_at(&dict(), &convert(&["ねこ。"]), 0);
        assert_eq!(surfaces(&c), ["猫。", "ねこ。", "ネコ。"]);
    }

    #[test]
    fn 記号で終わる区間は次の区間を送り仮名にしない() {
        let c = candidates_at(&dict(), &convert(&["か。", "く"]), 0);
        assert!(c.iter().all(|c| c.span == 1));
    }

    #[test]
    fn かな区間とそのまま出す区間はその文字列だけを候補にする() {
        let pieces = [
            Piece::kana("きょう"),
            Piece::literal("Emacs"),
            Piece::convert("か"),
        ];
        assert_eq!(
            candidates_at(&dict(), &pieces, 0),
            [Candidate::ranked("きょう", 1, 0)]
        );
        assert_eq!(
            candidates_at(&dict(), &pieces, 1),
            [Candidate::ranked("Emacs", 1, 0)]
        );
    }

    #[test]
    fn abbrev_区間は綴りそのままの見出しを引き最後に綴りのまま出す() {
        let c = candidates_at(&dict(), &[Piece::abbrev("emacs")], 0);
        assert_eq!(
            c,
            [
                Candidate::ranked("Ｅｍａｃｓ", 1, 0),
                Candidate::ranked("イーマックス", 1, 1),
                Candidate::kana("emacs"),
            ]
        );
        assert_eq!(
            candidates_at(&dict(), &[Piece::abbrev("vim")], 0),
            [Candidate::kana("vim")]
        );
    }

    #[test]
    fn 山括弧の印が付いた区間は接頭辞と接尾辞の見出しでも引く() {
        let pieces = [
            Piece::convert("お").with_marks(true, false),
            Piece::convert("かい").with_marks(false, true),
        ];
        let c = candidates_at(&dict(), &pieces, 0);
        assert!(c.contains(&Candidate::ranked("尾", 1, 0)));
        assert!(c.contains(&Candidate::ranked("御", 1, 0)));
        // `>` の次の区間は送り仮名にしない。
        assert!(c.iter().all(|c| c.span == 1));
        let c = candidates_at(&dict(), &pieces, 1);
        assert!(c.contains(&Candidate::ranked("貝", 1, 0)));
        assert!(c.contains(&Candidate::ranked("会", 1, 0)));
    }

    #[test]
    fn 接尾辞は残りのかなを続けられるが接頭辞は読み全体で引く() {
        let c = candidates_at(
            &dict(),
            &[Piece::convert("かいは").with_marks(false, true)],
            0,
        );
        assert!(c.contains(&Candidate::ranked("会は", 1, 0)));
        let c = candidates_at(
            &dict(),
            &[Piece::convert("おは").with_marks(true, false)],
            0,
        );
        assert!(!c.iter().any(|c| c.surface == "御は"));
    }

    #[test]
    fn 変換区間でない次の区間は送り仮名にしない() {
        let pieces = [Piece::convert("か"), Piece::kana("く")];
        assert!(
            candidates_at(&dict(), &pieces, 0)
                .iter()
                .all(|c| c.span == 1)
        );
    }
}
