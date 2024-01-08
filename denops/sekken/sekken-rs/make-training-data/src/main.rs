use std::io::Write;

use anyhow::Result;
use bzip2::read::MultiBzDecoder;

use sekken_core::util::is_japanese;

fn main() -> Result<()> {
    let input = std::io::stdin();
    let input = std::io::BufReader::new(input);
    let input = MultiBzDecoder::new(input);
    let mut input = utf8_read::Reader::new(input);

    let jps = input.into_iter().flatten().filter(|ch| is_japanese(*ch));

    let mut output = std::io::stdout().lock();

    for jp in jps {
        write!(output, "{}", jp)?;
    }

    Ok(())
}
