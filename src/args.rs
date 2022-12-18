use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Debug, Subcommand)]
pub enum Mode {
    Run(RunArgs),
    GenPeerId {
        secret_key_path: String,
        public_key_path: String,
    },
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(short, long)]
    pub config: String,

    #[arg(short, long)]
    pub debug: bool,
}
