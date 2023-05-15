mod cli;
mod db;
mod log;
mod orchestrator;
mod prelude;
mod scraper;

use clap::{CommandFactory, Parser};
use cli::{Args, Command};
use db::Database;
use parking_lot::Mutex;
use regex::RegexSet;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::{
    io::{self, Write},
    sync::Arc,
    time::Duration,
};
use tracing::{debug, Level};

use crate::orchestrator::RuntimeConfig;

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
        .map(|x| x.parse::<url::Url>().expect("Invalid seed url"))
        .collect();

    let whitelist = RegexSet::new(args.whitelist).expect("invalid whitelist regexes");
    let blacklist = RegexSet::new(args.blacklist).expect("invalid blacklist regexes");

    let sqlite_options = SqliteConnectOptions::new()
        .filename(&args.output_file)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let conn = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(sqlite_options)
        .await
        .expect("Can't open sqlite file");

    sqlx::query(include_str!("../sqls/INIT.sql"))
        .execute(&conn)
        .await
        .expect("Failed to initialize sqlite file schema");

    let db = Database::new(conn);
    let config = RuntimeConfig::new(args.max_parallel_requests.into(), whitelist, blacklist);
    let config = Arc::new(Mutex::new(config));

    let mut orchestrator = orchestrator::Orchestrator::new(src, config.clone(), db);
    let operation = orchestrator.start(args.include_db_links);
    tokio::pin!(operation);
    let mut cli = cli::Repl::new();

    loop {
        if cli.is_closed() {
            (&mut operation).await?;
        } else {
            tokio::select! {
                x = &mut operation => {
                    println!("Done: {:?}", x);
                    x?;
                    break;
                },
                _ = cli.next_input() => {debug!("User input") }
            }

            cli.run(config.clone()).await?;
        }
    }

    println!("To Exit? Why am I not exiting? is tokio keeping me alive. I don't know"); // FIXME
    Ok(())
}
