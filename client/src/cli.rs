use clap::Parser;
use clap::Subcommand;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(alias = "d")]
    Download { urls: Vec<String> },

    #[command(alias = "p")]
    Pause { id: usize },

    #[command(alias = "r")]
    Resume { id: usize },

    #[command(alias = "x")]
    Cancel { id: usize },

    #[command(alias = "l")]
    List,
}
