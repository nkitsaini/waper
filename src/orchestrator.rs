use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;

use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use patricia_tree::PatriciaSet;
use regex::RegexSet;
use url::Url;

use tokio::sync::mpsc;

use crate::config::ConfigReciever;
use crate::db::Database;
use crate::prelude::*;
use crate::scraper;

/// Used to run and controll the craping
/// let runner = Orchestrator::new(config)
/// runner.init(); // read state from db if necessary
///
///
/// runner.run().await ; // wait till run finishes
///
/// You can pause/resume/cancel operation using normal future primitives.
/// For example using: https://tokio.rs/tokio/tutorial/select#resuming-an-async-operation
/// let operation = runner.run();
/// tokio::pin!(operation);
/// loop {
///     tokio::select! {
///         _ = &mut operation => {};   
///         _ = tokio::time::sleep(dur) => {};   
///     }
///     // Do something every `dur` and in the next loop everything will be resumed from the left of state.
/// }
/// tokio::race!(a, timeout(10))
/// As soon as `a` is dropped aka cancelled, the processing will stop. Any outgoing requests will be cancelled.
///
/// Use: https://tokio.rs/tokio/tutorial/select#resuming-an-async-operation
/// let a =  runner.run(); // start where left of
///
pub struct Orchestrator {
    seed_urls: Vec<Url>,
    config: Arc<Mutex<RuntimeConfig>>,

    queue_rx: mpsc::UnboundedReceiver<Url>,
    queue_tx: mpsc::UnboundedSender<Url>,

    request_client: reqwest::Client,

    // URIs which have already been added to queue_rx
    // So do not need to be added again.
    noticed_uris: Arc<Mutex<PatriciaSet>>,

    // tasks: futures::stream::FuturesUnordered<BoxFuture<'static, ()>>,
    tasks: futures::stream::FuturesUnordered<BoxFuture<'static, anyhow::Result<()>>>,

    db: Database,
}

#[derive(Debug, Clone)]
pub struct RateLimit {
    max_parallel_requests: u64,
}

#[derive(Debug, Clone)]
pub struct Filter {
    pub blacklist_re: RegexSet,
    pub whitelist_re: RegexSet,
}

impl Filter {
    pub fn is_match(&self, value: &str) -> bool {
        debug!(value, "Filter match");
        if self.blacklist_re.is_match(value) || !self.whitelist_re.is_match(value) {
            debug!(
                value,
                "Did not match: {:?}, {:?}", self.blacklist_re, self.whitelist_re
            );
            false
        } else {
            debug!(value, "Passed filter");
            true
        }
    }
}
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub rate_limit: RateLimit,
    pub filter: Filter,
}
impl RuntimeConfig {
    pub fn new(limit: RateLimit, whitelist_re: RegexSet, blacklist_re: RegexSet) -> Self {
        Self {
            rate_limit: limit,
            filter: Filter {
                whitelist_re,
                blacklist_re,
            },
        }
    }
}

impl Orchestrator {
    pub fn new(seed_urls: Vec<Url>, config: Arc<Mutex<RuntimeConfig>>, db: Database) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        Self {
            seed_urls,
            config,
            queue_rx,
            queue_tx,
            request_client: reqwest::ClientBuilder::new()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            noticed_uris: Arc::new(Mutex::new(PatriciaSet::new())),
            tasks: futures::stream::FuturesUnordered::new(),
            db,
        }
    }

    pub async fn start(&mut self, include_unprocessed_from_db: bool) -> anyhow::Result<()> {
        debug!("Starting orchestrator");
        let mut seed_links = self.seed_urls.clone();
        if include_unprocessed_from_db {
            seed_links.append(&mut self.db.get_unprocessed_links().await?);
        }

        let split_point =
            (self.config.lock().rate_limit.max_parallel_requests as usize).min(seed_links.len());
        // Schedule seed links
        for link in &seed_links[..split_point] {
            info!("Scheduling {}", link);
            self.tasks
                .push(Self::scrape_link(self.create_context(), link.clone()).boxed());
        }
        for link in &seed_links[split_point..] {
            self.queue_tx.send(link.clone())?;
        }
        self.noticed_uris
            .lock()
            .extend(seed_links.iter().map(|x| x.to_string()));

        self.db.add_to_links(self.seed_urls.clone()).await?;

        while let Some(task) = self.tasks.next().await {
            if let Err(e) = task {
                error!("Error: {:?}", e);
                // continue even if error as task completion might have freed the `RateLimit` pool
            }
            while self.tasks.len() < self.config.lock().rate_limit.max_parallel_requests as usize {
                // error drop: The error can never be `TryRecvError::Disconnected`
                // as we always have a reference to queue_tx in `Self`
                if let Ok(x) = self.queue_rx.try_recv() {
                    info!("Scheduling {}", x);
                    self.tasks
                        .push(Self::scrape_link(self.create_context(), x).boxed());
                } else {
                    break;
                }
            }
        }
        Ok(())
    }

    async fn scrape_link(context: ScraperContext, url: Url) -> anyhow::Result<()> {
        let scrape_result = scraper::scrap_links(&url, context.request_client).await;

        debug!("Visited {}", url);

        let scrape_result = match scrape_result {
            Ok(r) => {
                context
                    .db
                    .add_to_results(url.clone(), r.html.clone())
                    .await?;
                r
            }
            Err(e) => {
                context
                    .db
                    .add_to_errors(url.clone(), format!("{e:?}"))
                    .await?;
                Err(e).context(format!("Failed to fetch webpage for uri: {url}"))?;
                unreachable!();
            }
        };

        let mut links_to_add = vec![];
        {
            // No async/heavy operation after this,
            // safe to take the lock
            let mut noticed_uris_lock = context.noticed_uris.lock();
            for link in scrape_result.links {
                // TODO: too many to_string operations
                // benchmark and move to passing strings around instead if required.
                if !context.config.lock().filter.is_match(link.as_str()) {
                    debug!("Does not match filter: {}", link);
                    continue;
                }

                if noticed_uris_lock.contains(&link.to_string()) {
                    debug!("Already Noticed: {}", link);
                    continue;
                }
                debug!("Found: {}", link);

                noticed_uris_lock.insert(&link.to_string());
                links_to_add.push(link.clone());
                // Self::add_to_links(vec![link.clone()], context.outfile.clone()).await?;
                context
                    .queue_tx
                    .send(link)
                    .expect("reciever should never be dropped as long as scrapes are running");
            }
        }
        context.db.add_to_links(links_to_add).await?;
        Ok(())
    }

    fn create_context(&self) -> ScraperContext {
        let queue_tx = self.queue_tx.clone();
        ScraperContext {
            config: self.config.clone(),
            noticed_uris: self.noticed_uris.clone(),
            request_client: self.request_client.clone(),
            db: self.db.clone(),
            queue_tx,
        }
    }
}

impl From<u64> for RateLimit {
    fn from(value: u64) -> Self {
        RateLimit {
            max_parallel_requests: value,
        }
    }
}

struct ScraperContext {
    config: Arc<Mutex<RuntimeConfig>>,
    noticed_uris: Arc<Mutex<PatriciaSet>>,
    request_client: reqwest::Client,
    queue_tx: mpsc::UnboundedSender<Url>,
    db: Database,
}
