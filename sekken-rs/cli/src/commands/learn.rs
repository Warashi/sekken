//! 過去に書いた文をまとめて本人の n-gram に数える。
//!
//! 本人の n-gram は変換で確定した文から数えるので、使い始めは空で効かない。本人がこれまでに書いた文を
//! 先に数えておくと、英数字を含む自分の文の先頭一致が 73.9% から 80.1% に上がった
//! （sekken-train の experiments/2026-10-13-personal-seed）。
//!
//! server は本人の n-gram をメモリに持ち、終了時に `--personal` に書く。server が動いている間に
//! このコマンドで同じファイルに書くと、server の終了時に上書きされて消えるので、server を止めてから使う。

use std::io::BufRead as _;
use std::path::PathBuf;

use anyhow::Result;
use sekken_model::personal::Personal;
use sekken_model::tokenizer::Tokenizer;

#[derive(clap::Args)]
pub struct Args {
    /// vibrato 辞書（system.dic.zst）。server と同じものを使う（語の分け方が数え方になる）
    #[arg(long)]
    dic: PathBuf,
    /// 本人の n-gram のファイル。あれば読んで数え足し、無ければ作る
    #[arg(long)]
    personal: PathBuf,
    /// `--personal` を新しく作るときに数える次数。既にあるファイルはその次数のまま読む
    #[arg(long, default_value_t = 3, value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..))]
    personal_order: usize,
}

/// 標準入力の文（1 行 1 文）を数えて `--personal` に書く。
pub fn run(args: Args) -> Result<()> {
    let tokenizer = Tokenizer::load(&args.dic)?;
    let mut personal = Personal::load_or_new(&args.personal, args.personal_order)?;
    let mut n = 0usize;
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        let sentence = line.trim();
        if sentence.is_empty() {
            continue;
        }
        sekken_model::scorer::learn(&mut personal, &tokenizer, sentence);
        n += 1;
    }
    personal.save_to(&args.personal)?;
    eprintln!(
        "sekken: learned {n} sentences into {}",
        args.personal.display()
    );
    Ok(())
}
