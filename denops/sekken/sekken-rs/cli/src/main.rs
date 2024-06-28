use std::env;
use std::fs::File;
use std::path::Path;

use sekken_converter::kana::KanaTable;
use sekken_dict::BTreeDict;
use sekken_lattice::Lattice;
use sekken_model::CompactModel;
use sekken_segmenter::SKK;

use anyhow::Context as _;
use anyhow::Result;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let dict = &args[1];
    let dict = Path::new(dict);
    let dict = File::open(dict).context("failed to open dictionary file")?;

    let dict = BTreeDict::from_skk_json(dict).context("failed to load dictionary file")?;

    let model = &args[2];
    let model = Path::new(model);
    let model = File::open(model).context("failed to open model file")?;
    let model = CompactModel::load(model).context("failed to load model file")?;

    let segconverter = SKK::<KanaTable>::default();

    println!("ready!");

    for line in std::io::stdin().lines() {
        let mut lattice = Lattice::new(&line?).context("failed to build lattice")?;
        lattice
            .build(&segconverter, &dict)
            .context("failed to bulid lattice")?;

        let result = lattice
            .viterbi(&model)
            .context("failed to caluclate viterbi path")?;

        let result = result
            .iter()
            .map(|n| n.surface.clone())
            .collect::<Vec<_>>()
            .concat();

        println!("{:?}", result);
    }

    return Ok(());
}
