use anyhow::Context;
use sqlx::sqlite;
use url::Url;

#[derive(Clone)]
pub struct Database {
    conn: sqlite::SqlitePool,
}

impl Database {
    pub fn new(conn: sqlite::SqlitePool) -> Self {
        Self { conn }
    }
    pub async fn add_to_links(&self, urls: Vec<Url>) -> anyhow::Result<()> {
        let mut tx = self.conn.begin().await?;
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

    pub async fn add_to_results(&self, url: Url, html: String) -> anyhow::Result<()> {
        let url_string = url.to_string();
        sqlx::query!(
            "INSERT INTO results (url, content) VALUES (?, ?)",
            url_string,
            html
        )
        .execute(&self.conn)
        .await
        .context(format!("Failed to insert in sqlite db for uri: {url}"))?;
        Ok(())
    }

    pub async fn add_to_errors(&self, url: Url, msg: String) -> anyhow::Result<()> {
        let url_string = url.to_string();
        sqlx::query!(
            "INSERT INTO errors (url, msg) VALUES (?, ?)",
            url_string,
            msg
        )
        .execute(&self.conn)
        .await
        .context(format!(
            "Failed to insert error in sqlite db for uri: {url}"
        ))?;
        Ok(())
    }

    pub async fn get_unprocessed_links(&self) -> anyhow::Result<Vec<Url>> {
        let results = sqlx::query!(
            "
			SELECT url FROM links
			WHERE
				url NOT IN (SELECT url FROM results) AND
				url NOT IN (SELECT url FROM errors)
			ORDER BY time",
        )
        .fetch_all(&self.conn)
        .await
        .context("Failed to fetch links from sqlite db".to_string())?;

        let mut rv = vec![];
        for result in results {
            let _uri = result.url.parse::<Url>().context("Invalid Url in DB")?;
            rv.push(_uri);
        }
        Ok(rv)
    }
}
