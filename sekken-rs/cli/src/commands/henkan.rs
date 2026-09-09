use anyhow::Result;

use sekken_cli::engine::EngineArgs;

#[derive(clap::Args)]
pub struct Args {
    #[command(flatten)]
    engine: EngineArgs,
    /// 候補数
    #[arg(long, default_value_t = 5)]
    top: usize,
    /// ローマ字入力
    inputs: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    let engine = args.engine.build()?;
    for input in &args.inputs {
        println!("{input}");
        for (i, c) in engine.henkan(input, args.top).iter().enumerate() {
            println!("  {}: {c}", i + 1);
        }
    }
    Ok(())
}
