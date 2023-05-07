use std::path::PathBuf;
use clap::Parser;

// Using tricks to make default subcommand work from: https://github.com/clap-rs/clap/issues/975
/// Program to scrape websites and save html to a sqlite file.
/// Example: waper --whitelist "https://example.com/.*" --whitelist "https://www.iana.org/domains/example" -s "https://example.com/"
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Option<Command>,

    #[clap(flatten)]
    pub scrape_args: ScrapeArgs,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// This is also default command, so it's optional to include in args.
    Scrape(ScrapeArgs),
    /// Print shell completion script
    Completion(CompletionArgs),
}


#[derive(Debug, clap::Args)]
pub struct CompletionArgs {
    pub shell: clap_complete::Shell
}

#[derive(Debug, clap::Args)]
pub struct ScrapeArgs {
    /// whitelist regexes: only these urls will be scanned other then seeds
    #[arg(short, long, default_value = ".*")]
    pub whitelist: Vec<String>,

    /// blacklist regexes: these urls will never be scanned
    /// By default nothing will be blacklisted
    #[arg(short, long)]
    pub blacklist: Vec<String>,

    /// Links to start with
    #[arg(short, long)]
    pub seed_links: Vec<String>,

    /// Sqlite output file
    #[arg(short, long, default_value = "waper_out.sqlite")]
    pub output_file: PathBuf,

    /// Sqlite output file
    #[arg(short, long, default_value_t = 5)]
    pub max_parallel_requests: u64,

    /// Will also include unprocessed links from `links` table in db
    /// if present. Helpful when you want to continue the scraping from
    /// a previously unfinished session.
    #[arg(short, long, default_value_t = false)]
    pub include_db_links: bool,

    /// Should verbose (debug) output
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
}

