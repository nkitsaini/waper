use std::sync::Arc;


use anyhow::Context;

use futures::{future::BoxFuture};
use futures::{FutureExt, StreamExt};
use http::Uri;
use patricia_tree::PatriciaSet;
use regex::RegexSet;

use tokio::sync::mpsc;


use crate::prelude::*;
use crate::scraper;

pub struct Orchestrator {
    seed_urls: Vec<Uri>,
    blacklist_re: Arc<RegexSet>,
    whitelist_re: Arc<RegexSet>,
    limits: RateLimit,

    queue_rx: mpsc::UnboundedReceiver<Uri>,
    queue_tx: mpsc::UnboundedSender<Uri>,

    request_client: reqwest::Client,

    // Contains all uris, even if fetch failed
    visited_uris: Arc<Mutex<PatriciaSet>>,

    // URIs which have already been added to queue_rx
    // So do not need to be added again.
    noticed_uris: Arc<Mutex<PatriciaSet>>,

    // tasks: futures::stream::FuturesUnordered<BoxFuture<'static, ()>>,
    tasks: futures::stream::FuturesUnordered<BoxFuture<'static, anyhow::Result<()>>>,

    outfile: sqlx::sqlite::SqlitePool,
}

pub struct RateLimit {
    max_parallel_requests: u64,
}
struct ScraperContext {
    blacklist_re: Arc<RegexSet>,
    whitelist_re: Arc<RegexSet>,
    visited_uris: Arc<Mutex<PatriciaSet>>,
    noticed_uris: Arc<Mutex<PatriciaSet>>,
    request_client: reqwest::Client,
    queue_tx: mpsc::UnboundedSender<Uri>,
    outfile: sqlx::sqlite::SqlitePool,
}
// unsafe impl Send for ScraperContext {}

impl Orchestrator {
    pub fn new(
        seed_urls: Vec<Uri>,
        blacklist_re: RegexSet,
        whitelist_re: RegexSet,
        limits: RateLimit,
        outfile: sqlx::sqlite::SqlitePool,
    ) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        Self {
            seed_urls,
            blacklist_re: Arc::new(blacklist_re),
            whitelist_re: Arc::new(whitelist_re),
            limits,
            queue_rx,
            queue_tx,
            request_client: reqwest::Client::new(),
            visited_uris: Arc::new(Mutex::new(PatriciaSet::new())),
            noticed_uris: Arc::new(Mutex::new(PatriciaSet::new())),
            tasks: futures::stream::FuturesUnordered::new(),
            outfile,
        }
    }

    fn create_context(&self) -> ScraperContext {
        let queue_tx = self.queue_tx.clone();
        ScraperContext {
            blacklist_re: self.blacklist_re.clone(),
            whitelist_re: self.whitelist_re.clone(),
            visited_uris: self.visited_uris.clone(),
            noticed_uris: self.noticed_uris.clone(),
            request_client: self.request_client.clone(),
            outfile: self.outfile.clone(),
            queue_tx,
        }
    }

    pub async fn start(&mut self) {
        debug!("Starting orchestrator");
        for link in &self.seed_urls {
            info!("Scheduling {}", link);
            self.tasks
                .push(Self::scrape_link(self.create_context(), link.clone()).boxed());
        }
        self.noticed_uris
            .lock()
            .extend(self.seed_urls.iter().map(|x| x.to_string()));

        while let Some(task) = self.tasks.next().await {
            if let Err(e) = task {
                error!("Error: {:?}", e);
                // continue even if error as task completion might have freed the `RateLimit` pool
            }
            while self.tasks.len() < self.limits.max_parallel_requests as usize {
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
    }

    async fn scrape_link(context: ScraperContext, url: Uri) -> anyhow::Result<()> {
        let scrape_result = scraper::scrap_links(&url, context.request_client).await;

        // TODO: too many to_string operations
        // benchmark and move to passing strings around instead if required.
        context.visited_uris.lock().insert(url.to_string());
        debug!("Visited {}", url);

        // add to visited even if fech fails to avoid
        // continous failure calls
        let scrape_result =
            scrape_result.context(format!("Failed to fetch webpage for uri: {url}"))?;

        let url_string = url.to_string();
        sqlx::query!(
            "INSERT INTO scrape_results (url, html) VALUES (?, ?)",
            url_string,
            scrape_result.html
        )
        .execute(&context.outfile)
        .await
        .context(format!("Failed to insert in sqlite db for uri: {url}"))?;
        // context.outfile.

        // No async/heavy operation after this,
        // safe to take the lock
        let mut noticed_uris_lock = context.noticed_uris.lock();

        for link in scrape_result.links {
            if context.blacklist_re.is_match(&link.to_string()) {
                debug!("Does not match blacklist: {}", link);
                continue;
            }
            if !context.whitelist_re.is_match(&link.to_string()) {
                debug!("Does not match whitelist: {}", link);
                continue;
            }

            if noticed_uris_lock.contains(&link.to_string()) {
                debug!("Already Noticed: {}", link);
                continue;
            }
            debug!("Found: {}", link);

            noticed_uris_lock.insert(&link.to_string());
            context
                .queue_tx
                .send(link)
                .expect("reciever should never be dropped as long as scrapes are running");
        }
        Ok(())
    }
}

impl From<u64> for RateLimit {
    fn from(value: u64) -> Self {
        RateLimit {
            max_parallel_requests: value,
        }
    }
}
