use select::predicate::Name;
use url::Url;

pub struct ScrapingResult {
    pub links: Vec<Url>,
    pub html: String,
}

pub async fn scrap_links(url: &Url, client: reqwest::Client) -> anyhow::Result<ScrapingResult> {
    let reqwest_url: reqwest::Url = reqwest::Url::parse(url.as_ref())?;
    let text = client.get(reqwest_url).send().await?.text().await?;
    let links = select::document::Document::from(text.as_str())
        .find(Name("a"))
        .filter_map(|n| {
            let value = n.attr("href")?;
            match url.join(value).ok() {
                Some(mut x) => {
                    // We don't care about fragements, multiple fragements are generally present in same page
                    // so this will make us crawl same page multiple times if left unchecked
                    x.set_fragment(None);
                    Some(x)
                }
                None => None,
            }
        })
        .collect::<Vec<_>>();

    Ok(ScrapingResult { links, html: text })
}
