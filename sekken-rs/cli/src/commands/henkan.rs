use anyhow::Result;

use sekken_cli::engine::{EngineArgs, henkan_roman};

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
        for (i, c) in henkan_roman(&engine, input, args.top).iter().enumerate() {
            println!("  {}: {c}", i + 1);
        }
    }
    // 打鍵を模した多数の入力で投機の反復数と段ごとの時間を見るため。
    if let Some(speculator) = &engine.speculator {
        eprintln!("{}", speculator.stats.borrow());
    }
    Ok(())
}
