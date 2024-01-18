use std::env;
use std::io::BufRead;

use glob::glob;

use anyhow::Context as _;
use anyhow::Result;

use rayon::prelude::*;

use sekken_core::util::is_japanese;
use sekken_model::normal::NormalModel;

mod wikijson;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let dir = args[2].clone() + "/*/wiki_*";

    let paths = glob(dir.as_str())?;
    let paths = paths.par_bridge().map(|path| path.unwrap());
    let paths = paths.filter(|path| path.is_file());
    let files = paths.map(|path| {
        std::fs::File::open(path)
            .context("Failed to open file")
            .unwrap()
    });
    let lines = files.flat_map(|file| std::io::BufReader::new(file).lines().par_bridge());
    let texts = lines
        .map(|line| {
            let text = serde_json::from_str::<wikijson::WikiJSON>(&line.unwrap())
                .unwrap()
                .text;
            let text = text
                .chars()
                .filter(|c| is_japanese(*c))
                .collect::<Vec<char>>();

            text
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();

    let mut model = NormalModel::new();

    for text in texts {
        text.iter()
            .zip(text.iter().skip(1))
            .for_each(|(c1, c2)| model.increment_bigram_cost(*c1, *c2));
    }

    let model = model.compact();

    let mut output = std::fs::File::create(args[1].clone())?;
    model.save(&mut output)?;

    Ok(())
}
