use std::sync::Arc;


use anyhow::Context;

use futures::{future::BoxFuture};
use futures::{FutureExt, StreamExt};
use http::Uri;
use patricia_tree::PatriciaSet;
use regex::RegexSet;

use sqlx::sqlite;
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
            noticed_uris: self.noticed_uris.clone(),
            request_client: self.request_client.clone(),
            outfile: self.outfile.clone(),
            queue_tx,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        debug!("Starting orchestrator");
        for link in &self.seed_urls {
            info!("Scheduling {}", link);
            self.tasks
                .push(Self::scrape_link(self.create_context(), link.clone()).boxed());
        }
        self.noticed_uris
            .lock()
            .extend(self.seed_urls.iter().map(|x| x.to_string()));
		Self::add_to_links(self.seed_urls.clone(), &self.outfile).await?;

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
		Ok(())
    }

    async fn scrape_link(context: ScraperContext, url: Uri) -> anyhow::Result<()> {
        let scrape_result = scraper::scrap_links(&url, context.request_client).await;

        debug!("Visited {}", url);

        let scrape_result =
            match scrape_result{
				Ok(r) => {
					Self::add_to_results(url.clone(), r.html.clone(), &context.outfile).await?;
					r
				},
				Err(e) => {
					Self::add_to_errors(url.clone(), format!("{e:?}"), &context.outfile).await?;
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
				links_to_add.push(link.clone());
				// Self::add_to_links(vec![link.clone()], context.outfile.clone()).await?;
				context
					.queue_tx
					.send(link)
					.expect("reciever should never be dropped as long as scrapes are running");
			}
		}
		Self::add_to_links(links_to_add, &context.outfile).await?;
        Ok(())
    }

	async fn add_to_links(urls: Vec<Uri>, db_conn: &sqlite::SqlitePool) -> anyhow::Result<()> {
		let mut tx = db_conn.begin().await?;
		for url in urls {
			// TODO: please make this performant (benchmark!!)
			// see: https://github.com/launchbadge/sqlx/issues/294
			let url_string = url.to_string();
			let query = sqlx::query!("INSERT INTO links (url) VALUES (?)", url_string);
			query.execute(&mut tx).await?;
		}
		tx.commit().await?;
		Ok(())
	}

	async fn add_to_results(url: Uri, html: String, db_conn: &sqlite::SqlitePool) -> anyhow::Result<()> {
		let url_string = url.to_string();
        sqlx::query!(
            "INSERT INTO results (url, content) VALUES (?, ?)",
            url_string,
            html
        )
        .execute(db_conn)
        .await
        .context(format!("Failed to insert in sqlite db for uri: {url}"))?;
		Ok(())
	}

	async fn add_to_errors(url: Uri, msg: String, db_conn: &sqlite::SqlitePool) -> anyhow::Result<()> {
		let url_string = url.to_string();
        sqlx::query!(
            "INSERT INTO errors (url, msg) VALUES (?, ?)",
            url_string,
            msg
        )
        .execute(db_conn)
        .await
        .context(format!("Failed to insert error in sqlite db for uri: {url}"))?;
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
