use http::Uri;
use select::predicate::Name;

pub struct ScrapingResult {
    pub links: Vec<Uri>,
    pub html: String,
}

pub async fn scrap_links(url: &Uri, client: reqwest::Client) -> anyhow::Result<ScrapingResult> {
    let reqwest_url: reqwest::Url = reqwest::Url::parse(&url.to_string())?;
    let text = client.get(reqwest_url).send().await?.text().await?;
    // TODO: implement
    let links = select::document::Document::from(text.as_str())
        .find(Name("a"))
        .filter_map(|n| {
            let value = n.attr("href")?;
            let mut uri: Uri = value.parse().ok()?;
            if uri.host().is_none() {
                let mut parts = uri.into_parts();
                parts.authority = url.authority().cloned();
                parts.scheme = url.scheme().cloned();
                uri = Uri::from_parts(parts).ok()?;
            }
            Some(uri)
        })
        .collect::<Vec<_>>();

    return Ok(ScrapingResult { links, html: text });
}
