use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "deep-dive",
    about = "A Docker/OCI image layer explorer TUI tool"
)]
pub struct Cli {
    /// The image URI to explore
    pub image: String,

    /// Path to configuration file
    #[arg(short = 'c', long = "config")]
    pub config: Option<PathBuf>,

    /// Increase verbosity (can be repeated)
    #[arg(short = 'v', action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,
}
