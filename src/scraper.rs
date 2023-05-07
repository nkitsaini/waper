use select::predicate::Name;
use url::Url;

pub struct ScrapingResult {
    pub links: Vec<Url>,
    pub html: String,
}


pub async fn scrap_links(url: &Url, client: reqwest::Client) -> anyhow::Result<ScrapingResult> {
    let reqwest_url: reqwest::Url = reqwest::Url::parse(&url.to_string())?;
    let text = client.get(reqwest_url).send().await?.text().await?;
    // TODO: implement
    let links = select::document::Document::from(text.as_str())
        .find(Name("a"))
        .filter_map(|n| {
            let value = n.attr("href")?;
            url.join(value).ok()
        })
        .collect::<Vec<_>>();

    Ok(ScrapingResult { links, html: text })
}
