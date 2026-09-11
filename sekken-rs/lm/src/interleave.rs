//! 読みと出力を区間ごとに交互に並べる形。
//!
//! 「読み全体 \t 出力全体」だと読みが 1 字伸びるたびに出力側の文脈が全部変わり、
//! 前の変換で読んだ状態を出力側で使い回せない。区間ごとに交互に並べれば、
//! 先頭の区間の列は後ろに何を足しても変わらないので、読み終えた状態を
//! 次の変換に持ち越せる。
//!
//! 区間は漢字の並びの先頭で切り、後ろに続くかなは同じ区間に含める
//! （今日も | 晴天なり。）。送り仮名や助詞を漢字と同じ区間に置くと、
//! 漢字を出す位置でその読み（ヨム → 読む）を見られる。区間の分け方は
//! 表層と読みの文字列だけから決まるので、学習データを作る側と採点する側が
//! 同じ規則で揃う。

use std::collections::HashSet;

use sekken_core::kana::hira2kata_char;

/// 区間の読みの前に置く字。
pub const READING: char = '\u{1e}';
/// 区間の出力の前に置く字。
pub const OUTPUT: char = '\t';

/// 表層を「変換される並び」と「読みにそのまま現れる並び」に分けたときの 1 並び。
struct Run {
    converted: bool,
    chars: Vec<char>,
}

/// 読みにそのまま現れず、変換で決まる字か。漢字と、漢字扱いの記号・小書きのカ・ケ。
fn is_kanji(c: char) -> bool {
    matches!(
        c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x3005 | 0x3006 | 0x3007 | 0x30F5 | 0x30F6
    )
}

/// かなと長音か（中黒は含めない）。
fn is_kana(c: char) -> bool {
    matches!(c as u32, 0x3041..=0x3096 | 0x309D | 0x309E | 0x30A1..=0x30FA | 0x30FC..=0x30FE)
        && !is_kanji(c)
}

/// 読みに現れないことがある記号か（中黒や感嘆符は読みから落ちることがある）。
fn is_symbol(c: char) -> bool {
    !is_kana(c) && !is_kanji(c) && !c.is_alphanumeric()
}

/// 表層を並びに分ける。数字や英字は読みにそのまま現れることも（英語の綴り）
/// 読みが付くことも（1993 → センキュウヒャクキュウジュウサン）あるので、
/// `words_converted` で漢字と同じ扱いにするか選ぶ。
fn runs(surface: &str, words_converted: bool) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for c in surface.chars() {
        let converted = is_kanji(c) || (words_converted && !is_kana(c) && !is_symbol(c));
        match runs.last_mut() {
            Some(run) if run.converted == converted => run.chars.push(c),
            _ => runs.push(Run {
                converted,
                chars: vec![c],
            }),
        }
    }
    runs
}

/// 当てはめの 1 要素。そのまま現れる並びは字ごと、変換される並びは並びごと。
enum Elem {
    Literal(char),
    Converted,
}

/// 並びを読みに当てる。要素と読みの位置の組で失敗を覚え、同じ組を二度探さない。
struct Matcher<'a> {
    /// (並びの番号, 要素)。
    elems: Vec<(usize, Elem)>,
    reading: &'a [char],
    failed: HashSet<(usize, usize)>,
}

impl Matcher<'_> {
    /// `elems[i..]` を `reading[pos..]` に当て、各要素が消費した長さを返す。
    /// そのまま現れる字は一致させ（記号は読みに無ければ飛ばす）、
    /// 変換される並びは短い方から試す。
    fn go(&mut self, i: usize, pos: usize) -> Option<Vec<usize>> {
        let Some((_, elem)) = self.elems.get(i) else {
            return (pos == self.reading.len()).then(Vec::new);
        };
        if self.failed.contains(&(i, pos)) {
            return None;
        }
        let found = match elem {
            Elem::Literal(c) => {
                let c = *c;
                let matched = self.reading.get(pos) == Some(&hira2kata_char(c));
                let found = matched.then(|| self.go(i + 1, pos + 1)).flatten();
                match found {
                    Some(mut v) => {
                        v.insert(0, 1);
                        Some(v)
                    }
                    None if is_symbol(c) => self.go(i + 1, pos).map(|mut v| {
                        v.insert(0, 0);
                        v
                    }),
                    None => None,
                }
            }
            Elem::Converted => (1..=(self.reading.len() - pos)).find_map(|n| {
                let mut v = self.go(i + 1, pos + n)?;
                v.insert(0, n);
                Some(v)
            }),
        };
        if found.is_none() {
            self.failed.insert((i, pos));
        }
        found
    }

    /// 各並びが消費した読みの長さ。
    fn run_lengths(&mut self, run_count: usize) -> Option<Vec<usize>> {
        let consumed = self.go(0, 0)?;
        let mut lengths = vec![0; run_count];
        for ((run, _), n) in self.elems.iter().zip(consumed) {
            lengths[*run] += n;
        }
        Some(lengths)
    }
}

fn match_runs(runs: &[Run], reading: &[char]) -> Option<Vec<usize>> {
    let mut elems = Vec::new();
    for (k, run) in runs.iter().enumerate() {
        if run.converted {
            elems.push((k, Elem::Converted));
        } else {
            elems.extend(run.chars.iter().map(|&c| (k, Elem::Literal(c))));
        }
    }
    Matcher {
        elems,
        reading,
        failed: HashSet::new(),
    }
    .run_lengths(runs.len())
}

/// 表層 `surface` と読み `reading`（カタカナ）を区間に分け、(読み, 表層) の列を返す。
/// 読みが表層に当てはまらなければ `None`。
pub fn align(surface: &str, reading: &str) -> Option<Vec<(String, String)>> {
    let reading: Vec<char> = reading.chars().collect();
    if surface.is_empty() || reading.is_empty() {
        return None;
    }
    // 数字や英字はまずそのまま現れるものとして当て、駄目なら読みが付くものとして当てる。
    let (runs, lengths) = [false, true].into_iter().find_map(|words_converted| {
        let runs = runs(surface, words_converted);
        let lengths = match_runs(&runs, &reading)?;
        Some((runs, lengths))
    })?;
    let mut segments: Vec<(String, String)> = Vec::new();
    let mut pos = 0;
    for (run, n) in runs.iter().zip(lengths) {
        let read: String = reading[pos..pos + n].iter().collect();
        let text: String = run.chars.iter().collect();
        pos += n;
        // 並びは交互に現れるので、そのまま現れる並びの前は変換される並びか文頭。
        // 変換される並びの後ろのかなは同じ区間に含め、文頭のかなは独立の区間にする。
        match segments.last_mut() {
            Some((r, s)) if !run.converted => {
                r.push_str(&read);
                s.push_str(&text);
            }
            _ => segments.push((read, text)),
        }
    }
    Some(segments)
}

/// 学習・採点に使う 1 行。区間ごとに `READING` 読み `OUTPUT` 出力を並べる。
/// 読みが表層に当てはまらなければ全体を 1 区間にする。
pub fn build(reading: &str, output: &str) -> String {
    try_build(reading, output).unwrap_or_else(|| join(&[(reading, output)]))
}

/// 読みが表層に当てはまるときだけ `build` と同じ行を返す。
pub fn try_build(reading: &str, output: &str) -> Option<String> {
    let segments = align(output, reading)?;
    let segments: Vec<(&str, &str)> = segments
        .iter()
        .map(|(r, s)| (r.as_str(), s.as_str()))
        .collect();
    Some(join(&segments))
}

fn join(segments: &[(&str, &str)]) -> String {
    let mut line = String::new();
    for (read, text) in segments {
        line.push(READING);
        line.push_str(read);
        line.push(OUTPUT);
        line.push_str(text);
    }
    line
}

/// 行の中で出力の字がある位置（出力の i 文字目 → 行の何文字目か）。
/// 交互でない行は全部の字が出力。
pub fn output_positions(line: &str) -> Vec<usize> {
    let chars: Vec<char> = line.chars().collect();
    if !chars.contains(&READING) {
        return (0..chars.len()).collect();
    }
    let mut in_output = false;
    let mut positions = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        match c {
            READING => in_output = false,
            OUTPUT => in_output = true,
            _ if in_output => positions.push(i),
            _ => {}
        }
    }
    positions
}

/// 行の各字が損失（採点）に入るか。出力の字と、区間の終わりを示す `READING` は
/// 入り、読みの字と `OUTPUT` は入らない。`READING` の無い行は `OUTPUT` の後ろ
/// だけ、どちらも無い行は全部が入る。
pub fn loss_mask(line: &str) -> Vec<bool> {
    let chars: Vec<char> = line.chars().collect();
    if !chars.contains(&READING) {
        let start = chars.iter().position(|&c| c == OUTPUT).map_or(0, |i| i + 1);
        return (0..chars.len()).map(|i| i >= start).collect();
    }
    let mut in_output = false;
    chars
        .iter()
        .map(|&c| match c {
            READING => {
                in_output = false;
                true
            }
            OUTPUT => {
                in_output = true;
                false
            }
            _ => in_output,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aligned(surface: &str, reading: &str) -> Vec<(String, String)> {
        align(surface, reading).unwrap()
    }

    fn pairs(v: &[(String, String)]) -> Vec<(&str, &str)> {
        v.iter().map(|(r, s)| (r.as_str(), s.as_str())).collect()
    }

    #[test]
    fn 漢字の先頭で切り後ろのかなを含める() {
        let v = aligned("今日も晴天なり。", "キョウモセイテンナリ。");
        assert_eq!(
            pairs(&v),
            [("キョウモ", "今日も"), ("セイテンナリ。", "晴天なり。")]
        );
    }

    #[test]
    fn 文頭のかなは独立の区間になる() {
        let v = aligned("それは猫だ", "ソレハネコダ");
        assert_eq!(pairs(&v), [("ソレハ", "それは"), ("ネコダ", "猫だ")]);
    }

    #[test]
    fn 送り仮名は漢字と同じ区間に入る() {
        let v = aligned("本を読む", "ホンヲヨム");
        assert_eq!(pairs(&v), [("ホンヲ", "本を"), ("ヨム", "読む")]);
    }

    #[test]
    fn 途中にかなを挟む語は漢字ごとに割れる() {
        let v = aligned("取り扱い", "トリアツカイ");
        assert_eq!(pairs(&v), [("トリ", "取り"), ("アツカイ", "扱い")]);
    }

    #[test]
    fn 同じかなが繰り返されても順に当てる() {
        let v = aligned("生き生き", "イキイキ");
        assert_eq!(pairs(&v), [("イキ", "生き"), ("イキ", "生き")]);
    }

    #[test]
    fn 末尾のかなは末尾に当てる() {
        // 「キ」は 1 字目にもあるが、末尾の「き」は読みの末尾に当てる。
        let v = aligned("気付き", "キヅキ");
        assert_eq!(pairs(&v), [("キヅキ", "気付き")]);
    }

    #[test]
    fn カタカナと記号は読みにそのまま現れる() {
        let v = aligned("猫、ネコ。", "ネコ、ネコ。");
        assert_eq!(pairs(&v), [("ネコ、ネコ。", "猫、ネコ。")]);
    }

    #[test]
    fn 数字と英字はそのまま現れなければ読みの付く字として当てる() {
        let v = aligned("1993年に、", "センキュウヒャクキュウジュウサンネンニ、");
        assert_eq!(
            pairs(&v),
            [("センキュウヒャクキュウジュウサンネンニ、", "1993年に、")]
        );
        let v = aligned("(Die sieben Raben)や", "(Die sieben Raben)ヤ");
        assert_eq!(
            pairs(&v),
            [("(Die sieben Raben)ヤ", "(Die sieben Raben)や")]
        );
        let v = aligned("ZZトップの", "ジージートップノ");
        assert_eq!(pairs(&v), [("ジージートップノ", "ZZトップの")]);
    }

    #[test]
    fn 読みから落ちた記号は飛ばして当てる() {
        let v = aligned("ナチス・ドイツの", "ナチスドイツノ");
        assert_eq!(pairs(&v), [("ナチスドイツノ", "ナチス・ドイツの")]);
        let v = aligned("ナチス・ドイツの", "ナチス・ドイツノ");
        assert_eq!(pairs(&v), [("ナチス・ドイツノ", "ナチス・ドイツの")]);
        let v = aligned("「パタリロ!」が", "「パタリロ」ガ");
        assert_eq!(pairs(&v), [("「パタリロ」ガ", "「パタリロ!」が")]);
    }

    #[test]
    fn 小書きのケは変換される字として扱う() {
        let v = aligned("霞ヶ関へ", "カスミガセキヘ");
        assert_eq!(pairs(&v), [("カスミガセキヘ", "霞ヶ関へ")]);
    }

    #[test]
    fn 読みが当てはまらなければ_none() {
        assert!(align("猫が鳴く", "ネコガ").is_none());
        assert!(align("猫が鳴く", "ネコノナク").is_none());
        assert!(align("猫", "").is_none());
        assert!(align("", "ネコ").is_none());
    }

    #[test]
    fn 行は区間ごとに読みと出力を交互に並べる() {
        assert_eq!(
            build("ソレハネコダ", "それは猫だ"),
            "\u{1e}ソレハ\tそれは\u{1e}ネコダ\t猫だ"
        );
    }

    #[test]
    fn 当てはまらない行は全体を_1_区間にする() {
        assert_eq!(
            build("ネコノナク", "猫が鳴く"),
            "\u{1e}ネコノナク\t猫が鳴く"
        );
        assert_eq!(try_build("ネコノナク", "猫が鳴く"), None);
        assert_eq!(
            try_build("ネコダ", "猫だ").as_deref(),
            Some("\u{1e}ネコダ\t猫だ")
        );
    }

    #[test]
    fn 損失は出力の字と区間の終わりに掛かる() {
        let line = "\u{1e}ネコ\t猫\u{1e}ガ\tが";
        assert_eq!(
            loss_mask(line),
            [true, false, false, false, true, true, false, false, true]
        );
    }

    #[test]
    fn 出力の字の位置を引ける() {
        assert_eq!(output_positions("\u{1e}ネコ\t猫\u{1e}ガ\tが"), [4, 8]);
        assert_eq!(output_positions("猫が"), [0, 1]);
    }

    #[test]
    fn 交互でない行はタブの後ろだけ_タブも無ければ全部() {
        assert_eq!(loss_mask("ネコ\t猫"), [false, false, false, true]);
        assert_eq!(loss_mask("猫が"), [true, true]);
    }
}
