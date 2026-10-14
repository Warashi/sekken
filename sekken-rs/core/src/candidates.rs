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
    /// literal の直後の区間の、読みをそのままひらがなにした候補か。かな罰則の代わりに
    /// literal の後のかな罰則を課す（`Weights::literal_kana_penalty`）。
    pub after_literal: bool,
    /// ユーザー辞書の候補か。使う本人が足した語で、モデルが知らない語でも
    /// 分かち書きの内部コストを払わない。
    pub user: bool,
}

impl Candidate {
    fn kana(surface: impl Into<String>) -> Candidate {
        Candidate {
            surface: surface.into(),
            span: 1,
            rank: 0,
            kana: true,
            after_literal: false,
            user: false,
        }
    }

    fn ranked(surface: impl Into<String>, span: usize, rank: usize) -> Candidate {
        Candidate {
            surface: surface.into(),
            span,
            rank,
            kana: false,
            after_literal: false,
            user: false,
        }
    }

    fn user(surface: impl Into<String>, rank: usize) -> Candidate {
        Candidate {
            surface: surface.into(),
            span: 1,
            rank,
            kana: false,
            after_literal: false,
            user: true,
        }
    }
}

/// ユーザー辞書と SKK 辞書の送りなし見出し `key` を引き、ユーザー辞書の候補を先に
/// `out` に足す。同じ表記が両方にあればユーザー辞書の側だけを残す。
fn push_okuri_nasi(
    out: &mut Vec<Candidate>,
    dict: &Dictionary,
    user: &Dictionary,
    key: &str,
    suffix: &str,
) {
    let first = out.len();
    for (rank, surface) in user.okuri_nasi(key).iter().enumerate() {
        out.push(Candidate::user(format!("{surface}{suffix}"), rank));
    }
    for (rank, surface) in dict.okuri_nasi(key).iter().enumerate() {
        let candidate = Candidate::ranked(format!("{surface}{suffix}"), 1, rank);
        if !out[first..].iter().any(|c| c.surface == candidate.surface) {
            out.push(candidate);
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
/// 送りなしと abbrev の見出しは `user`（ユーザー辞書）を先に引く。
/// literal は `;` でしか閉じられず、直後の助詞（`'Emacs;wo`）も辞書を引く区間になってひらがなと示す
/// 手段が無いので、literal の直後の区間のひらがなの候補には印を付け、軽いかな罰則にする。
pub fn candidates_at(
    dict: &Dictionary,
    user: &Dictionary,
    pieces: &[Piece],
    index: usize,
) -> Vec<Candidate> {
    let piece = &pieces[index];
    match piece.kind {
        Kind::Convert => {}
        Kind::Abbrev => return abbrev(dict, user, &piece.text),
        Kind::Kana => return vec![Candidate::ranked(piece.text.clone(), 1, 0)],
        Kind::Literal => {
            let mut out = vec![Candidate::ranked(piece.text.clone(), 1, 0)];
            // 数字の literal と次の区間をまとめて、数字を書き換えた数値変換の候補（３つ、三つ）にする。
            if is_number(&piece.text)
                && let Some(next) = pieces.get(index + 1)
                && next.kind == Kind::Convert
            {
                out.extend(numeric(dict, &piece.text, &next.text, false));
            }
            return out;
        }
    }
    let (reading, symbols) = split_trailing_symbols(&piece.text);
    let mut out = Vec::new();

    // 数字の literal の後の区間は、数値変換の見出し（`#つ` の `#0つ`）を数字を打ったままの形で引く。
    if index > 0 && pieces[index - 1].kind == Kind::Literal && is_number(&pieces[index - 1].text) {
        out.extend(numeric(dict, &pieces[index - 1].text, &piece.text, true));
    }

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
        push_okuri_nasi(&mut out, dict, user, &yomi, &suffix);
        // 接頭辞・接尾辞の見出しは通常の見出しと同じ語を持つことがあるので、重複は足さない。
        let mut affix = |key: String| {
            for (rank, surface) in dict.okuri_nasi(&key).iter().enumerate() {
                let candidate = Candidate::ranked(format!("{surface}{suffix}"), 1, rank);
                if !out.iter().any(|c| c.surface == candidate.surface) {
                    out.push(candidate);
                }
            }
        };
        if piece.suffix {
            affix(format!(">{yomi}"));
        }
        // 接頭辞は読み全体が見出しで、後ろに残りのかなを続けない。
        if piece.prefix && rest.is_empty() {
            affix(format!("{yomi}>"));
        }
        // カタカナ語に助詞が続く `すこっとらんどは` のような入力のための候補。
        if !rest.is_empty() && split >= 2 {
            out.push(Candidate::kana(format!("{}{suffix}", hira2kata(&yomi))));
        }
    }

    let mut hiragana = Candidate::kana(piece.text.clone());
    hiragana.after_literal = index > 0 && pieces[index - 1].kind == Kind::Literal;
    out.push(hiragana);
    out.push(Candidate::kana(hira2kata(&piece.text)));
    out
}

/// 数値変換の対象になる literal（ASCII の数字だけ）。
fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// SKK の数値変換。数字の literal `number` の後に読み `text` の区間が続くとき、読みの頭を `#` 付きの
/// 見出し（`#つ`、`#こ`）で引き、候補の `#0`〜`#3` を数字に置き換える。
///
/// `kept` なら数字を打ったまま（`#0`）の候補だけを、数字を含めずに後ろの区間の候補として返す（3 は literal が
/// 出す）。そうでなければ数字を書き換える候補（`#2` 漢数字、`#3` 位取りの漢数字）を、literal と後ろの区間の
/// 2 区間を覆う候補として返す。打った半角の数字を残すことがほとんどなので、書き換える候補は打ったままの候補の
/// 数だけ順位を下げ、かなの候補と同じ罰則を課す。全角（`#1`）は出さない。半角で打った数字を全角にしたい場面は
/// ほぼ無く、罰則を課しても LM が選ぶことがある（2018年7月 → ７月）。`#4` 以降（再帰、大字、棋譜）も出さない。
fn numeric(dict: &Dictionary, number: &str, text: &str, kept: bool) -> Vec<Candidate> {
    let (reading, symbols) = split_trailing_symbols(text);
    let chars: Vec<char> = reading.chars().collect();
    let mut kept_out = Vec::new();
    let mut rewritten = Vec::new();
    for split in (1..=chars.len()).rev() {
        let yomi: String = chars[..split].iter().collect();
        let rest: String = chars[split..].iter().collect();
        if rest.starts_with(is_small_kana) {
            continue;
        }
        for template in dict.okuri_nasi(&format!("#{yomi}")) {
            let Some(i) = template.find('#') else {
                continue;
            };
            let (before, after) = template.split_at(i);
            let mut tail = after[1..].chars();
            let Some(kind) = tail.next() else {
                continue;
            };
            let after = tail.as_str();
            if after.contains('#') {
                continue;
            }
            let digits = match kind {
                '0' => {
                    // 数字の後がひらがなの候補（3つ）だけ。かなの候補には literal の後のかな罰則が掛かるので
                    // 引かないと「3角」に負ける。漢字の助数詞（個、回）は普通の辞書引きで出せ、ここで足すと
                    // 助詞を押しのける（1つか → 1束、4か → 4日）。
                    if before.is_empty() && after.starts_with(is_hiragana) {
                        kept_out.push(format!("{after}{rest}{symbols}"));
                    }
                    continue;
                }
                '2' => number.chars().map(kanji_digit).collect(),
                '3' => match kanji_number(number) {
                    Some(s) => s,
                    None => continue,
                },
                _ => continue,
            };
            rewritten.push(format!("{before}{digits}{after}{rest}{symbols}"));
        }
    }
    let mut out = Vec::new();
    if kept {
        for (rank, surface) in kept_out.into_iter().enumerate() {
            if !out.iter().any(|c: &Candidate| c.surface == surface) {
                out.push(Candidate::ranked(surface, 1, rank));
            }
        }
    } else {
        // 打った数字を書き換える候補には、かなの候補と同じ罰則を課す。順位を下げるだけでは、literal から
        // 後ろの区間への接続を数えない分だけ安くなり、2018年7月 が ７月 になる。
        let offset = kept_out.len();
        for (rank, surface) in rewritten.into_iter().enumerate() {
            if !out.iter().any(|c: &Candidate| c.surface == surface) {
                out.push(Candidate {
                    kana: true,
                    ..Candidate::ranked(surface, 2, offset + rank)
                });
            }
        }
    }
    out
}

const KANJI_DIGITS: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];

fn kanji_digit(c: char) -> char {
    c.to_digit(10).map_or(c, |d| KANJI_DIGITS[d as usize])
}

/// 位取りの漢数字（1024 → 千二十四）。16 桁を超える数は `None`。
fn kanji_number(number: &str) -> Option<String> {
    let n: u64 = number.parse().ok()?;
    if n == 0 {
        return Some("〇".to_string());
    }
    if n >= 10_000_000_000_000_000 {
        return None;
    }
    let mut out = String::new();
    let mut rest = n;
    for (unit, name) in [
        (1_000_000_000_000u64, "兆"),
        (100_000_000, "億"),
        (10_000, "万"),
        (1, ""),
    ] {
        let part = rest / unit;
        rest %= unit;
        if part == 0 {
            continue;
        }
        for (place, mark) in [(1000, '千'), (100, '百'), (10, '十')] {
            let d = part / place % 10;
            if d > 1 || (d == 1 && unit > 1 && place == 1000) {
                out.push(KANJI_DIGITS[d as usize]);
            }
            if d > 0 {
                out.push(mark);
            }
        }
        let d = part % 10;
        if d > 0 {
            out.push(KANJI_DIGITS[d as usize]);
        }
        out.push_str(name);
    }
    Some(out)
}

/// abbrev 区間の候補。綴りそのままの見出しを引き、無ければ綴りのまま出す。
fn abbrev(dict: &Dictionary, user: &Dictionary, text: &str) -> Vec<Candidate> {
    let mut out = Vec::new();
    push_okuri_nasi(&mut out, dict, user, text, "");
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
>かい /会/貝/
emacs /Ｅｍａｃｓ/イーマックス/
つの /角/
#つ /#1つ/#0つ/#2つ/#3つ/
#こ /#3個/#0個/#4個/
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
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["ねこ"]), 0);
        assert_eq!(surfaces(&c), ["猫", "ねこ", "ネコ"]);
    }

    #[test]
    fn 次の区間を送り仮名として送りあり候補を出す() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["か", "く"]), 0);
        assert_eq!(c[0], Candidate::ranked("書く", 2, 0));
        assert_eq!(c[1], Candidate::ranked("掛く", 2, 1));
        assert_eq!(c[2], Candidate::ranked("可", 1, 0));
    }

    #[test]
    fn 区間内の残りを送り仮名として送りあり候補を出す() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["かきます"]), 0);
        assert!(c.contains(&Candidate::ranked("書きます", 1, 0)));
        assert!(c.contains(&Candidate::ranked("可きます", 1, 0)));
    }

    #[test]
    fn っで始まる送り仮名はっの次の字で送りあり候補を引く() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["とった"]), 0);
        assert!(c.contains(&Candidate::ranked("採った", 1, 0)));
        assert!(c.contains(&Candidate::ranked("取った", 1, 1)));
    }

    #[test]
    fn 次の区間がっで始まっても送りあり候補を引く() {
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &convert(&["と", "った"]),
            0,
        );
        assert_eq!(c[0], Candidate::ranked("採った", 2, 0));
    }

    #[test]
    fn 綴りが違っても同じかなの送り仮名は両方の見出しを引く() {
        // ち は ti の t と chi の c の両方で登録されている。重複は 1 つにする。
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["とち"]), 0);
        let okuri: Vec<&Candidate> = c.iter().filter(|c| !c.kana).collect();
        assert_eq!(
            okuri.iter().map(|c| c.surface.as_str()).collect::<Vec<_>>(),
            ["採ち", "取ち"]
        );
    }

    #[test]
    fn 母音で始まる送り仮名も引く() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["おおい"]), 0);
        assert!(c.contains(&Candidate::ranked("多い", 1, 0)));
    }

    #[test]
    fn 打ち終えていない綴りの英字は送り仮名の子音として使う() {
        // `KaK` と打っている途中。エディタは末尾の子音を変換せずに送る。
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["か", "k"]), 0);
        assert_eq!(c[0], Candidate::ranked("書k", 2, 0));
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["かk"]), 0);
        assert!(c.contains(&Candidate::ranked("書k", 1, 0)));
    }

    #[test]
    fn 読みの前方一致で辞書を引き残りをかなにする() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["ねこは"]), 0);
        assert_eq!(surfaces(&c), ["猫は", "ネコは", "ねこは", "ネコハ"]);
    }

    #[test]
    fn 小書きの字の手前では読みを切らない() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["きゃ"]), 0);
        assert_eq!(surfaces(&c), ["きゃ", "キャ"]);
    }

    #[test]
    fn カタカナの前方一致にかなを続けた候補を出す() {
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &convert(&["すこっとらんどは"]),
            0,
        );
        assert!(c.contains(&Candidate::kana("スコットランドは")));
    }

    #[test]
    fn 記号を含む区間はまとめてかなにする() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["ねこ・"]), 0);
        assert!(c.contains(&Candidate::ranked("猫・", 1, 0)));
        assert!(c.contains(&Candidate::kana("ねこ・")));
    }

    #[test]
    fn 途中の長音記号は読みの一部として扱う() {
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &convert(&["たーげっとにする"]),
            0,
        );
        assert!(c.contains(&Candidate::kana("ターゲットにする")));
    }

    #[test]
    fn 末尾の記号は読みから外して候補に付け直す() {
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["ねこ。"]), 0);
        assert_eq!(surfaces(&c), ["猫。", "ねこ。", "ネコ。"]);
    }

    #[test]
    fn 記号で終わる区間は次の区間を送り仮名にしない() {
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &convert(&["か。", "く"]),
            0,
        );
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
            candidates_at(&dict(), &Dictionary::default(), &pieces, 0),
            [Candidate::ranked("きょう", 1, 0)]
        );
        assert_eq!(
            candidates_at(&dict(), &Dictionary::default(), &pieces, 1),
            [Candidate::ranked("Emacs", 1, 0)]
        );
    }

    #[test]
    fn 数字の_literal_の後の区間は数値変換の見出しを数字を打ったままの形で引く() {
        let pieces = [Piece::literal("3"), Piece::convert("つの")];
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 1);
        // 数字は literal が出すので、後ろの区間の候補は数字を含めない。かな罰則は掛からない。
        assert_eq!(c[0], Candidate::ranked("つの", 1, 0));
        assert!(c.contains(&Candidate::ranked("角", 1, 0)));
        // 数字でない literal の後では引かない。
        let pieces = [Piece::literal("a"), Piece::convert("つの")];
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 1);
        assert_eq!(c[0], Candidate::ranked("角", 1, 0));
    }

    #[test]
    fn 数字の_literal_は次の区間と合わせて数字を書き換えた候補を出す() {
        let pieces = [Piece::literal("3"), Piece::convert("つの")];
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 0);
        // 打ったままの候補（#0）の数だけ順位を下げる。全角（#1）と #4 以降は出さない。
        assert_eq!(
            c,
            [
                Candidate::ranked("3", 1, 0),
                Candidate {
                    kana: true,
                    ..Candidate::ranked("三つの", 2, 1)
                },
            ]
        );
        let pieces = [Piece::literal("12"), Piece::convert("こ")];
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 0);
        assert_eq!(surfaces(&c), ["12", "十二個"]);
        // 漢字の助数詞は打ったままの候補にしない（普通の辞書引きで出す）。
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 1);
        assert!(!c.contains(&Candidate::ranked("個", 1, 0)));
    }

    #[test]
    fn 位取りの漢数字() {
        assert_eq!(kanji_number("0").as_deref(), Some("〇"));
        assert_eq!(kanji_number("10").as_deref(), Some("十"));
        assert_eq!(kanji_number("1024").as_deref(), Some("千二十四"));
        assert_eq!(kanji_number("20000").as_deref(), Some("二万"));
        assert_eq!(kanji_number("10000000").as_deref(), Some("一千万"));
        assert_eq!(kanji_number("100000000").as_deref(), Some("一億"));
        assert_eq!(kanji_number("99999999999999999"), None);
    }

    #[test]
    fn abbrev_区間は綴りそのままの見出しを引き最後に綴りのまま出す() {
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &[Piece::abbrev("emacs")],
            0,
        );
        assert_eq!(
            c,
            [
                Candidate::ranked("Ｅｍａｃｓ", 1, 0),
                Candidate::ranked("イーマックス", 1, 1),
                Candidate::kana("emacs"),
            ]
        );
        assert_eq!(
            candidates_at(&dict(), &Dictionary::default(), &[Piece::abbrev("vim")], 0),
            [Candidate::kana("vim")]
        );
    }

    #[test]
    fn 山括弧の印が付いた区間は接頭辞と接尾辞の見出しでも引く() {
        let pieces = [
            Piece::convert("お").with_marks(true, false),
            Piece::convert("かい").with_marks(false, true),
        ];
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 0);
        assert!(c.contains(&Candidate::ranked("尾", 1, 0)));
        assert!(c.contains(&Candidate::ranked("御", 1, 0)));
        // `>` の次の区間は送り仮名にしない。
        assert!(c.iter().all(|c| c.span == 1));
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 1);
        assert!(c.contains(&Candidate::ranked("貝", 1, 0)));
        assert!(c.contains(&Candidate::ranked("会", 1, 0)));
        // 通常の見出しにもある語は重ねて出さない。
        assert_eq!(c.iter().filter(|c| c.surface == "貝").count(), 1);
    }

    #[test]
    fn 接尾辞は残りのかなを続けられるが接頭辞は読み全体で引く() {
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &[Piece::convert("かいは").with_marks(false, true)],
            0,
        );
        assert!(c.contains(&Candidate::ranked("会は", 1, 0)));
        let c = candidates_at(
            &dict(),
            &Dictionary::default(),
            &[Piece::convert("おは").with_marks(true, false)],
            0,
        );
        assert!(!c.iter().any(|c| c.surface == "御は"));
    }

    #[test]
    fn ユーザー辞書の候補を印付きで先に出し同じ表記は重ねない() {
        let user = Dictionary::parse(
            "\
;; okuri-nasi entries.
ねこ /猫/根子/
",
        );
        let c = candidates_at(&dict(), &user, &convert(&["ねこ"]), 0);
        assert_eq!(
            c,
            [
                Candidate::user("猫", 0),
                Candidate::user("根子", 1),
                Candidate::kana("ねこ"),
                Candidate::kana("ネコ"),
            ]
        );
    }

    #[test]
    fn ユーザー辞書は残りのかなと_abbrev_でも引く() {
        let user = Dictionary::parse(
            "\
;; okuri-nasi entries.
ねこ /根子/
emacs /Emacs/
",
        );
        let c = candidates_at(&dict(), &user, &convert(&["ねこは"]), 0);
        assert_eq!(c[0], Candidate::user("根子は", 0));
        let c = candidates_at(&dict(), &user, &[Piece::abbrev("emacs")], 0);
        assert_eq!(c[0], Candidate::user("Emacs", 0));
        assert_eq!(c[1], Candidate::ranked("Ｅｍａｃｓ", 1, 0));
    }

    #[test]
    fn 変換区間でない次の区間は送り仮名にしない() {
        let pieces = [Piece::convert("か"), Piece::kana("く")];
        assert!(
            candidates_at(&dict(), &Dictionary::default(), &pieces, 0)
                .iter()
                .all(|c| c.span == 1)
        );
    }

    #[test]
    fn literal_の直後の区間はひらがなの候補にだけ印を付ける() {
        let pieces = [Piece::literal("Emacs"), Piece::convert("お")];
        let c = candidates_at(&dict(), &Dictionary::default(), &pieces, 1);
        let marked: Vec<&str> = c
            .iter()
            .filter(|c| c.after_literal)
            .map(|c| c.surface.as_str())
            .collect();
        assert_eq!(marked, ["お"]);
        let c = candidates_at(&dict(), &Dictionary::default(), &convert(&["お"]), 0);
        assert!(c.iter().all(|c| !c.after_literal));
    }
}
