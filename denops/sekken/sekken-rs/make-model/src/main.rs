use std::env;
use std::io::BufRead;

use glob::glob;

use anyhow::Context as _;
use anyhow::Result;

use rayon::prelude::*;

use sekken_core::util::is_japanese;

mod wikijson;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let dir = args[1].clone() + "/*/wiki_*";

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
            let text = text.chars().filter(|c| is_japanese(*c)).collect::<Vec<char>>();

            text
        })
        .filter(|text| text.len() > 0)
        .collect::<Vec<_>>();

    println!("{}", texts.len());

    Ok(())
}
