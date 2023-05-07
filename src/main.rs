mod log;
mod orchestrator;
mod prelude;
mod scraper;
mod cli;

use regex::RegexSet;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tracing::Level;
use clap::{Parser, CommandFactory};
use cli::{Args, Command};
use std::io;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if let Some(Command::Completion(shell)) = args.command {
        let mut cmd = Args::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell.shell, &mut cmd, name, &mut io::stdout());
        return Ok(());
    }

    // If no command is provided or the `scrape` command is provided
    // we want to scrape

    let args = args.scrape_args;
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

    let sqlite_options = SqliteConnectOptions::new()
        .filename(&args.output_file)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let outfile = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(sqlite_options)
        .await
        .expect("Can't open sqlite file");

    sqlx::query(include_str!("../sqls/INIT.sql"))
        .execute(&outfile)
        .await
        .expect("Failed to initialize sqlite file schema");

    let mut orchestrator =
        orchestrator::Orchestrator::new(src, blacklist, whitelist, args.max_parallel_requests.into(), outfile);

    orchestrator.start(args.include_db_links).await
}
