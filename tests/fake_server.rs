//! Starts a fake server, serving from an In-Memory directory structure

use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    extract::Path,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    response::Response,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::net::SocketAddr;

fn get_pages() -> HashMap<String, Vec<String>> {
    let mut rv: HashMap<String, Vec<String>> = Default::default();
    // 1 > [120, 130]
    // 2 > [1, 10..15]
    // 3 > [1, 10..15]
    //
    rv.insert("/".to_string(), (1..4).map(|x| format!("/{x}")).collect());
    rv.insert(
        "/1".to_string(),
        (1100..1900).map(|x| format!("/{x}")).collect(),
    );

    rv.insert(
        "/2".to_string(),
        (2100..2900).map(|x| format!("/{x}")).collect(),
    );

    // create recursion
    rv.get_mut("/2").unwrap().push("/1".to_string());
    rv.get_mut("/1").unwrap().push("/2".to_string());

    rv.insert(
        "/3".to_string(),
        (3001..3100).map(|x| format!("/{x}")).collect(),
    );

    rv
}

#[derive(Deserialize, Debug)]
struct PathVars {
    #[serde(default = "String::new")]
    page: String,
}

fn create_html(page_path: &str, links: &Vec<String>) -> String {
    let mut rv = page_path.to_string();
    rv += "<br />";
    for link in links {
        rv += &format!("<a href=\"{link}\"> {link} </a>");
    }
    rv
}

async fn page_return(path: Path<PathVars>) -> Response {
    let pages = get_pages(); // who cares for speed here?

    let key = format!("/{}", &path.page);
    let mut headers = HeaderMap::new();
    headers.append("Content-Type", HeaderValue::from_static("text/html"));

    tokio::time::sleep(Duration::from_secs(300)).await; // rust is too fast to notice without it

    (
        StatusCode::OK,
        headers,
        create_html(&key, pages.get(&key).unwrap_or(&vec![])),
    )
        .into_response()
}

/// Just a random server to test stuff against
#[ignore]
#[tokio::test]
async fn run_server() -> anyhow::Result<()> {
    let app = Router::new()
        .route("/*page", get(page_return))
        .route("/", get(page_return));
    let addr = SocketAddr::from(([127, 0, 0, 1], 10605));
    println!("Listening on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
    Ok(())
}
