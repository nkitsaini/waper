use anyhow::Context;
use parking_lot::Mutex;
use regex::RegexSet;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader, Lines, Stdin};

use clap::{Command, CommandFactory, FromArgMatches, Parser};
use std::{io::Write, path::PathBuf, sync::Arc, time::Duration};

use crate::orchestrator::RuntimeConfig;

#[derive(Parser, Debug)]
#[command(about="Repl to control waper runtime.", long_about = None)]
struct ReplParser {
    #[clap(subcommand)]
    pub command: ReplCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum ReplCommand {
    /// Exit repl and continue scraping
    Exit,
    /// Blacklist all future urls
    /// After this waper will only scan currently known urls
    BlacklistAll,
}

pub struct Repl {
    reader: Lines<BufReader<Stdin>>,
    closed: bool,
}

impl Repl {
    pub fn new() -> Self {
        let stdin = io::stdin();
        let reader = tokio::io::BufReader::new(stdin).lines();
        Self {
            reader,
            closed: false,
        }
    }

    pub async fn run(&mut self, config: Arc<Mutex<RuntimeConfig>>) -> anyhow::Result<()> {
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::io::stdout().flush()?; // flush any logs in queue
        self.print_help().await?;
        loop {
            // Wait for background futures to settle
            // hyper logs after the future poll stop, not sure if it is reqwest or hyper
            // only an issue for debug logs
            let command = match self.next_command().await? {
                Some(x) => x,
                None => break,
            };
            match command {
                ReplCommand::Exit => break,
                ReplCommand::BlacklistAll => {
                    let mut c = config.lock();
                    c.filter.blacklist_re = RegexSet::new([".*"]).unwrap();
                }
            };
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }
    /// Just wait for user to interact and discard the input
    pub async fn next_input(&mut self) -> Option<String> {
        if self.closed {
            return None;
        }
        match self.reader.next_line().await.unwrap() {
            Some(x) => Some(x),
            None => {
                self.closed = true;
                None
            }
        } // is it safe to unwrap stdin buffer?
    }

    fn get_command(&self) -> Command {
        ReplParser::command()
            .disable_help_flag(true)
            .multicall(true)
            .bin_name(" ") // TODO: use a proper repl implementation which can support Tab completions
                           // can we use gnu repl here? will that give vim mode?
    }

    pub async fn print_help(&self) -> anyhow::Result<()> {
        println!("===> Waper Repl");
        self.get_command().print_help()?;
        Ok(())
    }

    pub async fn next_command(&mut self) -> anyhow::Result<Option<ReplCommand>> {
        loop {
            print!("> ");
            std::io::stdout().flush()?;
            let input = match self.next_input().await {
                Some(x) => x,
                None => return Ok(None),
            };
            let input = match shlex::split(&input) {
                Some(x) => x,
                None => {
                    eprintln!("Cannot parse input.");
                    std::io::stderr().flush()?;
                    continue;
                }
            };
            let mut cmd = self.get_command();

            let command = cmd.clone().try_get_matches_from_mut(input);
            let mut command = match command {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("Invalid input.");
                    e.print()?;
                    std::io::stderr().flush()?;
                    std::io::stdout().flush()?;
                    continue;
                }
            };
            let command =
                ReplParser::from_arg_matches_mut(&mut command).map_err(|e| e.format(&mut cmd));
            match command {
                Ok(x) => {
                    return Ok(Some(x.command));
                }
                Err(e) => {
                    eprintln!("Invalid input.");
                    e.print()?;
                    std::io::stderr().flush()?;
                    std::io::stdout().flush()?;
                    continue;
                }
            }
        }
    }
}
