use std::env;
use std::io::BufRead;

use anyhow::Context as _;
use anyhow::Result;
use glob::glob;

mod wikijson;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let dir = args[1].clone() + "/*/wiki_*";

    let paths = glob(dir.as_str())?;
    let paths = paths
        .map(|path| path.context("Failed to read path"))
        .collect::<Result<Vec<_>>>()?;
    let paths = paths.into_iter().filter(|path| path.is_file());
    let files = paths
        .map(|path| std::fs::File::open(path).context("Failed to open file"))
        .collect::<Result<Vec<_>>>()?;
    let lines = files
        .into_iter()
        .flat_map(|file| std::io::BufReader::new(file).lines());
    let texts = lines.map(|line| {
        serde_json::from_str::<wikijson::WikiJSON>(&line.unwrap())
            .unwrap()
            .text
    });

    for text in texts {
        println!("{}", text);
    }

    Ok(())
}
