mod log;
mod orchestrator;
mod prelude;
mod scraper;

use std::path::PathBuf;

use clap::Parser;
use regex::RegexSet;
use sqlx::sqlite::SqlitePoolOptions;
use tracing::Level;

use std::io;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::os::unix;
use std::path::Path;

/// Program to scrape websites and save html to a sqlite file.
/// Example: waper --whitelist "https://example.com/.*" --whitelist "https://www.iana.org/domains/example" -s "https://example.com/"
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// whitelist regexes: only these urls will be scanned other then seeds
    #[arg(short, long)]
    whitelist: Vec<String>,

    // a^ matches nothing, hence default
    // https://stackoverflow.com/questions/940822/regular-expression-syntax-for-match-nothing
    //
    /// blacklist regexes: these urls will never be scanned
    /// By default nothing will be blacklisted
    #[arg(short, long, default_value = "a^")]
    blacklist: Vec<String>,

    /// Links to start with
    #[arg(short, long)]
    seed_links: Vec<String>,

    /// Sqlite output file
    #[arg(short, long, default_value = "waper_out.sqlite")]
    output_file: PathBuf,

    /// Should verbose (debug) output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn touch(path: &Path) -> io::Result<()> {
    match OpenOptions::new().create(true).write(true).open(path) {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    if args.verbose {
        log::init_logging(Level::DEBUG);
    } else {
        log::init_logging(Level::INFO);
    }
    let src: Vec<_> = args
        .seed_links
        .into_iter()
        .map(|x| x.parse::<http::Uri>().expect("Invalid seed url"))
        .collect();

    let whitelist = RegexSet::new(args.whitelist).expect("invalid whitelist regexes");
    let blacklist = RegexSet::new(args.blacklist).expect("invalid blacklist regexes");

    touch(&args.output_file).expect("Failed to touch sqlite file");

    let outfile = SqlitePoolOptions::new()
        .max_connections(3)
        .connect(&format!("sqlite://{}", args.output_file.to_str().unwrap()))
        .await
        .expect("Can't open sqlite file");
    sqlx::query(include_str!("../sqls/INIT.sql")).execute(&outfile).await.expect("Failed to initialize sqlite file schema");

    let mut orchestrator =
        orchestrator::Orchestrator::new(src, blacklist, whitelist, 5.into(), outfile);

    orchestrator.start().await;
}
