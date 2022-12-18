use clap::Parser;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Args {
    #[arg(short, long)]
    pub config: String,

    #[arg(short, long)]
    pub debug: bool,
}
