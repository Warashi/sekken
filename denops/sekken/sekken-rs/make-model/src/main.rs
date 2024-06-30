use std::io::{BufWriter, Write};

use anyhow::{Context, Result};

use sekken_model::NormalModel;

fn main() -> Result<()> {
    let input = std::io::stdin();
    let input = std::io::BufReader::new(input);
    let mut input = utf8_read::Reader::new(input);
    let mut pre: Option<char> = None;

    let mut model = NormalModel::new();

    for ch in input.into_iter().flatten() {
        model.increment_unigram_score(ch);

        if let Some(p) = pre {
            model.increment_bigram_score(p, ch);
        }

        pre = Some(ch);
    }
    let model = model;

    let output = std::io::stdout();
    let mut output = BufWriter::new(output);

    model
        .compact()
        .save(&mut output)
        .context("Failed to save model")?;

    output.flush().context("Failed to flush output")?;

    Ok(())
}
