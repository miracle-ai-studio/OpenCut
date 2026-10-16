use super::*;
use serde_json::json;

fn post_value(slug: &str) -> serde_json::Value {
    json!({"id":"source","slug":slug,"title":"Text <script> & \"quote\"","description":"Description & <tag>","content":"<p>Not embedded in metadata</p>","coverImage":"https://example.com/cover.jpg?a=1&b=2","publishedAt":"2026-10-16T08:30:00.000Z","authors":[{"name":"A & B"}]})
}
fn content() -> PublicContent {
    PublicContent::fixture(vec![
        (
            Key::Post("known".into()),
            Ok(json!({"post":post_value("known")})),
        ),
        (Key::Post("unavailable".into()), Err(Failure::Unavailable)),
        (
            Key::Page(1),
            Ok(
                json!({"posts":[post_value("known")],"pagination":{"currpage":1,"totalPages":1,"nextPage":null}}),
            ),
        ),
    ])
}
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("index.html"),"<!doctype html><html><head><title>Generic</title></head><body><div id=\"root\"></div></body></html>").unwrap();
    root
}
fn request(path: &str, method: Method) -> Request<()> {
    Request::builder()
        .method(method)
        .uri(path)
        .body(())
        .unwrap()
}
async fn body(response: Response) -> String {
    String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn first_html_contains_escaped_source_metadata_without_an_editor_cookie() {
    let directory = root();
    let path = directory.path().canonicalize().unwrap();
    let content = content();
    let response = serve(&path, &request("/blog/known", Method::GET), &content)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let html = body(response).await;
    assert!(html.contains("<title>Text &lt;script&gt; &amp; &quot;quote&quot; · OpenCut</title>"));
    assert!(html.contains("rel=\"canonical\"") && html.contains("/blog/known\""));
    assert!(html.contains("property=\"og:image\"") && html.contains("cover.jpg?a=1&amp;b=2"));
    assert!(html.contains("name=\"twitter:card\"") && html.contains("article:published_time"));
    assert!(
        !html.contains("<script>")
            && !html.contains("Not embedded")
            && !html.contains("<title>Generic")
    );
    let head = serve(&path, &request("/blog/known", Method::HEAD), &content)
        .await
        .unwrap();
    assert_eq!(
        head.headers()[header::CONTENT_LENGTH]
            .to_str()
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        html.len()
    );
    assert!(body(head).await.is_empty());
}

#[tokio::test]
async fn syntax_alone_cannot_make_missing_content_a_success_and_failures_are_distinct() {
    let directory = root();
    let path = directory.path().canonicalize().unwrap();
    let content = content();
    for route in [
        "/blog/missing-but-valid",
        "/blog/a%2fb",
        "/blog/a/b",
        "/changelog/999.999.999",
        "/app/public/v1/blog/missing",
    ] {
        assert_eq!(
            serve(&path, &request(route, Method::GET), &content)
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND,
            "{route}"
        );
    }
    assert_eq!(
        serve(&path, &request("/blog/unavailable", Method::GET), &content)
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_GATEWAY
    );
    let release = releases().remove(0);
    let response = serve(
        &path,
        &request(&format!("/changelog/{}", release.version), Method::GET),
        &content,
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let html = body(response).await;
    assert!(html.contains(&escape(&release.title)) && html.contains(&escape(&release.description)));
    assert_eq!(
        serve(
            &path,
            &request("/app/public/v1/blog?page=1&url=secret", Method::GET),
            &content
        )
        .await
        .unwrap()
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(
        serve(
            &path,
            &request("/editor/private-project", Method::GET),
            &content
        )
        .await
        .is_none()
    );
}

#[tokio::test]
async fn feeds_and_sitemap_are_public_escaped_complete_and_exclude_editor_routes() {
    let directory = root();
    let path = directory.path().canonicalize().unwrap();
    let content = content();
    let feed = serve(&path, &request("/rss.xml", Method::GET), &content)
        .await
        .unwrap();
    assert_eq!(feed.status(), StatusCode::OK);
    assert_eq!(
        feed.headers()[header::CONTENT_TYPE],
        "application/xml; charset=utf-8"
    );
    let feed = body(feed).await;
    assert!(feed.contains("<pubDate>Fri, 16 Oct 2026 08:30:00 GMT</pubDate>"));
    assert!(feed.contains("Text &lt;script&gt; &amp; &quot;quote&quot;"));
    let map = body(
        serve(&path, &request("/sitemap.xml", Method::GET), &content)
            .await
            .unwrap(),
    )
    .await;
    assert!(map.contains("/blog/known</loc>") && map.contains("/changelog/0.3.0</loc>"));
    assert!(!map.contains("/editor") && !map.contains("/projects") && !map.contains("/app/"));
    let robots = body(
        serve(&path, &request("/robots.txt", Method::GET), &content)
            .await
            .unwrap(),
    )
    .await;
    assert!(robots.contains("Sitemap:") && robots.contains("Disallow: /editor"));
    let failed = PublicContent::fixture(vec![(Key::Page(1), Err(Failure::Unavailable))]);
    assert_eq!(
        serve(&path, &request("/sitemap.xml", Method::GET), &failed)
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_GATEWAY
    );
}

#[test]
fn xml_dates_and_attributes_never_pass_invalid_markup_through() {
    assert_eq!(
        escape("a\u{0}\u{1}\u{fffe}<&\"'😀"),
        "a&lt;&amp;&quot;&apos;😀"
    );
    assert_eq!(
        rss_date("2024-02-29T00:00:00Z").as_deref(),
        Some("Thu, 29 Feb 2024 00:00:00 GMT")
    );
    for invalid in [
        "2025-02-29T00:00:00Z",
        "2026-10-16T24:00:00Z",
        "2026-10-16T00:00:00oopsZ",
        "2026-10-16T00:00:00.Z",
        "2026-10-16T00:00:00+00:00",
    ] {
        assert!(rss_date(invalid).is_none(), "{invalid}")
    }
}
