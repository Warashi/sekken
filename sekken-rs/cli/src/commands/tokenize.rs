use std::path::PathBuf;

use anyhow::Result;
use sekken_model::tokenizer::Tokenizer;

#[derive(clap::Args)]
pub struct Args {
    /// vibrato 辞書（system.dic.zst）
    #[arg(long)]
    dic: PathBuf,
    /// 分かち書きする文
    texts: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let tokenizer = Tokenizer::load(&args.dic)?;
    for text in &args.texts {
        let tokens: Vec<String> = tokenizer
            .tokenize(text)
            .into_iter()
            .map(|t| format!("{}({})", t.surface, t.reading.unwrap_or_default()))
            .collect();
        println!("{}", tokens.join(" / "));
    }
    Ok(())
}
