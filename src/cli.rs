use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "docsearch", version = "0.2.0", about = "Fast local document indexing & search with native Slint GUI")]
pub struct Cli {
    #[arg(short, long, help = "Path to custom config JSON file")]
    pub config: Option<String>,

    #[arg(long, help = "Run in background daemon mode without opening GUI window")]
    pub headless: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Search documents from the command line")]
    Search {
        #[arg(help = "Search query keyword")]
        query: String,
        #[arg(short, long, default_value = "10", help = "Maximum results to display")]
        limit: usize,
    },
    #[command(about = "Add a directory to the indexed folders list")]
    Add {
        #[arg(help = "Path to directory to index")]
        folder: String,
    },
    #[command(about = "List configured folders and index status")]
    List,
    #[command(about = "Trigger a full re-scan and indexing of all configured folders")]
    Reindex,
    #[command(name = "duplicates", alias = "dupes", about = "Find and report duplicate files")]
    Duplicates,
}
