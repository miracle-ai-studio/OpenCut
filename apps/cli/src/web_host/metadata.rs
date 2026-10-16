//! Classic public metadata, rendered before hydration with escaped source values.
use super::{
    public_content::{Content, Failure, Key, Post, PublicContent},
    *,
};
use serde::Deserialize;
use std::time::Duration;
const DESCRIPTION: &str =
    "A simple but powerful video editor that gets the job done. In your browser.";
const DEFAULT_ORIGIN: &str = "https://opencut.app";
const ROUTES: &[(&str, &str)] = &[
    ("/", "OpenCut"),
    ("/brand", "Brand assets"),
    ("/roadmap", "Roadmap"),
    ("/sponsors", "Sponsors"),
    ("/privacy", "Privacy Policy"),
    ("/terms", "Terms of Service"),
    ("/contributors", "Contributors"),
    ("/blog", "Blog"),
    ("/changelog", "Changelog"),
];
#[derive(Deserialize)]
struct Release {
    version: String,
    title: String,
    description: String,
    date: String,
}
fn releases() -> Vec<Release> {
    serde_json::from_str(include_str!("../../../web/src/site/releases.json"))
        .expect("shared static release catalog")
}
pub(super) fn escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars().filter(|c| matches!(*c, '\n' | '\r' | '\t' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')) {
        match c { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), '"' => out.push_str("&quot;"), '\'' => out.push_str("&apos;"), _ => out.push(c) }
    }
    out
}
fn origin() -> String {
    let value =
        std::env::var("OPENCUT_PUBLIC_SITE_ORIGIN").unwrap_or_else(|_| DEFAULT_ORIGIN.into());
    let value = value.trim_end_matches('/');
    value
        .parse::<axum::http::Uri>()
        .ok()
        .filter(|u| {
            u.scheme_str() == Some("https")
                && u.authority().is_some_and(|a| !a.as_str().contains('@'))
                && u.path() == "/"
                && u.query().is_none()
        })
        .map_or_else(|| DEFAULT_ORIGIN.into(), |_| value.into())
}
struct Metadata {
    title: String,
    description: String,
    path: String,
    image: String,
    article: bool,
    date: Option<String>,
    authors: Vec<String>,
}
impl Metadata {
    fn generic(path: &str, title: &str) -> Self {
        // Original page descriptions and illustrations from Classic app pages.
        let description = match path {
            "/blog" => {
                "Read the latest news and updates about OpenCut, the free and open-source video editor."
            }
            "/contributors" => {
                "Meet the amazing people who contribute to OpenCut, the free and open-source video editor."
            }
            "/sponsors" => {
                "Support OpenCut and help us build the future of free and open-source video editing."
            }
            "/privacy" => {
                "Learn how OpenCut handles your data and privacy. Our commitment to protecting your information while you edit videos."
            }
            "/terms" => {
                "OpenCut's Terms of Service. Fair, transparent terms for our free and open-source video editor."
            }
            "/roadmap" => {
                "See what's coming next for OpenCut - the free, open-source video editor that respects your privacy."
            }
            "/changelog" => "What's new in OpenCut",
            _ => DESCRIPTION,
        };
        let illustration = match path {
            "/roadmap" => "roadmap",
            "/changelog" => "changelog",
            _ => "default",
        };
        Self {
            title: if path == "/" {
                title.into()
            } else {
                format!("{title} · OpenCut")
            },
            description: description.into(),
            path: path.into(),
            image: format!("{}/open-graph/{illustration}.jpg", origin()),
            article: false,
            date: None,
            authors: Vec::new(),
        }
    }
    fn post(p: &Post) -> Self {
        Self {
            title: format!("{} · OpenCut", p.title),
            description: p.description.clone(),
            path: format!("/blog/{}", p.slug),
            image: p
                .cover
                .clone()
                .unwrap_or_else(|| format!("{}/open-graph/default.jpg", origin())),
            article: true,
            date: Some(p.date.clone()),
            authors: p.authors.iter().map(|author| author.name.clone()).collect(),
        }
    }
    fn head(&self) -> String {
        let title = escape(&self.title);
        let description = escape(&self.description);
        let url = escape(&format!("{}{}", origin(), self.path));
        let image = escape(&self.image);
        let mut head = format!(
            "<title>{title}</title><meta name=\"description\" content=\"{description}\"><link rel=\"canonical\" href=\"{url}\"><meta property=\"og:title\" content=\"{title}\"><meta property=\"og:description\" content=\"{description}\"><meta property=\"og:url\" content=\"{url}\"><meta property=\"og:image\" content=\"{image}\"><meta property=\"og:type\" content=\"{}\"><meta name=\"twitter:card\" content=\"summary_large_image\"><meta name=\"twitter:title\" content=\"{title}\"><meta name=\"twitter:description\" content=\"{description}\"><meta name=\"twitter:image\" content=\"{image}\">",
            if self.article { "article" } else { "website" }
        );
        if let Some(date) = &self.date {
            if rss_date(date).is_some() {
                head.push_str(&format!(
                    "<meta property=\"article:published_time\" content=\"{}\">",
                    escape(date)
                ));
            }
        }
        for author in &self.authors {
            head.push_str(&format!(
                "<meta property=\"article:author\" content=\"{}\">",
                escape(author)
            ));
        }
        head
    }
}
fn html(template: &str, metadata: &Metadata) -> String {
    let mut output = template.to_owned();
    if let Some(start) = output.find("<title>") {
        if let Some(end) = output[start..].find("</title>") {
            output.replace_range(start..start + end + 8, "");
        }
    }
    let head = metadata.head();
    if let Some(end) = output.find("</head>") {
        output.insert_str(end, &head)
    } else {
        output.insert_str(0, &head)
    }
    output
}
pub(super) fn response(
    status: StatusCode,
    mime: &'static str,
    body: String,
    head: bool,
) -> Response {
    let len = body.len();
    let mut r = (
        status,
        if head {
            Body::empty()
        } else {
            Body::from(body)
        },
    )
        .into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(len));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    r
}
fn failed(error: Failure, head: bool) -> Response {
    let status = error.status();
    response(
        status,
        "text/plain; charset=utf-8",
        if status == StatusCode::NOT_FOUND {
            "Public content not found."
        } else {
            "Public content is temporarily unavailable. Please retry."
        }
        .into(),
        head,
    )
}
async fn lookup(content: &PublicContent, key: Key) -> Result<std::sync::Arc<Content>, Failure> {
    tokio::time::timeout(Duration::from_secs(15), content.get(key))
        .await
        .map_err(|_| Failure::Unavailable)?
}
pub(super) async fn serve(
    root: &Path,
    request: &Request,
    content: &PublicContent,
) -> Option<Response> {
    let path = request.uri().path();
    let head = request.method() == Method::HEAD;
    let relevant = path == "/robots.txt"
        || path == "/rss.xml"
        || path == "/sitemap.xml"
        || path.starts_with("/app/public/v1/blog")
        || path.starts_with("/blog/")
        || path.starts_with("/changelog/")
        || ROUTES.iter().any(|(p, _)| *p == path);
    if !relevant {
        return None;
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return Some(StatusCode::METHOD_NOT_ALLOWED.into_response());
    }
    if path == "/robots.txt" {
        return Some(response(
            StatusCode::OK,
            "text/plain; charset=utf-8",
            format!(
                "User-agent: *\nAllow: /\nDisallow: /editor\nDisallow: /projects\nDisallow: /app/\nDisallow: /v1/\nSitemap: {}/sitemap.xml\n",
                origin()
            ),
            head,
        ));
    }
    if path.starts_with("/app/public/v1/blog") {
        let key = if path == "/app/public/v1/blog" {
            let page = match request.uri().query() {
                None => Some(1),
                Some(q) => q.strip_prefix("page=").and_then(|p| p.parse::<u32>().ok()),
            };
            match page {
                Some(p) => Key::Page(p),
                None => return Some(StatusCode::BAD_REQUEST.into_response()),
            }
        } else if let Some(slug) = path.strip_prefix("/app/public/v1/blog/") {
            Key::Post(slug.into())
        } else {
            return Some(StatusCode::NOT_FOUND.into_response());
        };
        return Some(match lookup(content, key).await {
            Ok(data) => response(
                StatusCode::OK,
                "application/json; charset=utf-8",
                data.json().to_string(),
                head,
            ),
            Err(e) => failed(e, head),
        });
    }
    if path == "/rss.xml" || path == "/sitemap.xml" {
        let posts = if path == "/rss.xml" {
            match lookup(content, Key::Page(1)).await {
                Ok(p) => match p.as_ref() {
                    Content::Page(p) => Ok(p.posts.clone()),
                    _ => Err(Failure::Invalid),
                },
                Err(e) => Err(e),
            }
        } else {
            tokio::time::timeout(Duration::from_secs(15), content.all_posts())
                .await
                .unwrap_or(Err(Failure::Unavailable))
        };
        return Some(match posts {
            Ok(posts) => response(
                StatusCode::OK,
                "application/xml; charset=utf-8",
                if path == "/rss.xml" {
                    rss(&posts, &origin())
                } else {
                    sitemap(&posts, &origin())
                },
                head,
            ),
            Err(e) => failed(e, head),
        });
    }
    let metadata = if let Some(slug) = path.strip_prefix("/blog/") {
        match lookup(content, Key::Post(slug.into())).await {
            Ok(p) => match p.as_ref() {
                Content::Post(p) => Metadata::post(p),
                _ => return Some(failed(Failure::Invalid, head)),
            },
            Err(e) => return Some(failed(e, head)),
        }
    } else if let Some(version) = path.strip_prefix("/changelog/") {
        let Some(release) = releases().into_iter().find(|r| r.version == version) else {
            return Some(failed(Failure::NotFound, head));
        };
        Metadata {
            title: format!(
                "{} ({}) - OpenCut Changelog",
                release.title, release.version
            ),
            description: release.description,
            path: path.into(),
            image: format!("{}/open-graph/changelog.jpg", origin()),
            article: false,
            date: Some(format!("{}T00:00:00Z", release.date)),
            authors: Vec::new(),
        }
    } else {
        Metadata::generic(path, ROUTES.iter().find(|(p, _)| *p == path).unwrap().1)
    };
    let file = root.join("index.html");
    let Ok(canonical) = tokio::fs::canonicalize(&file).await else {
        return Some(StatusCode::NOT_FOUND.into_response());
    };
    if !canonical.starts_with(root) {
        return Some(StatusCode::NOT_FOUND.into_response());
    }
    let Ok(size) = tokio::fs::metadata(&canonical).await else {
        return Some(StatusCode::NOT_FOUND.into_response());
    };
    if size.len() > 1024 * 1024 {
        return Some(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }
    let Ok(template) = tokio::fs::read_to_string(canonical).await else {
        return Some(StatusCode::INTERNAL_SERVER_ERROR.into_response());
    };
    Some(response(
        StatusCode::OK,
        "text/html; charset=utf-8",
        html(&template, &metadata),
        head,
    ))
}
// RFC3339 UTC values from the source become an RSS 2.0 RFC822 date. Invalid or
// non-UTC dates are omitted, never guessed or passed into executable markup.
fn rss_date(value: &str) -> Option<String> {
    if value.len() < 20 || !value.ends_with('Z') {
        return None;
    }
    let suffix = value.get(19..)?;
    if suffix != "Z"
        && !suffix
            .strip_prefix('.')
            .and_then(|s| s.strip_suffix('Z'))
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let b = value.as_bytes();
    if b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || b.get(10) != Some(&b'T')
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
    {
        return None;
    }
    let y: i32 = value.get(..4)?.parse().ok()?;
    let m: usize = value.get(5..7)?.parse().ok()?;
    let d: usize = value.get(8..10)?.parse().ok()?;
    let hh: u8 = value.get(11..13)?.parse().ok()?;
    let mm: u8 = value.get(14..16)?.parse().ok()?;
    let ss: u8 = value.get(17..19)?.parse().ok()?;
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=9999).contains(&y)
        || !(1..=12).contains(&m)
        || d == 0
        || d > days[m - 1]
        || hh > 23
        || mm > 59
        || ss > 59
    {
        return None;
    }
    let a = if m < 3 { y - 1 } else { y };
    let offsets = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let weekday = (a + a / 4 - a / 100 + a / 400 + offsets[m - 1] + d as i32) % 7;
    Some(format!(
        "{}, {d:02} {} {y:04} {hh:02}:{mm:02}:{ss:02} GMT",
        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday as usize],
        [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
        ][m - 1]
    ))
}
fn rss(posts: &[Post], origin: &str) -> String {
    let root = escape(origin);
    let mut out = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><rss version=\"2.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:media=\"http://search.yahoo.com/mrss/\"><channel><title>OpenCut Blog</title><link>{root}/blog</link><description>{}</description><language>en</language><image><url>{root}/open-graph/default.jpg</url><title>OpenCut Blog</title><link>{root}/blog</link></image>",
        escape(DESCRIPTION)
    );
    for post in posts {
        let url = escape(&format!("{origin}/blog/{}", post.slug));
        out.push_str(&format!("<item><title>{}</title><link>{url}</link><guid isPermaLink=\"true\">{url}</guid><description>{}</description>",escape(&post.title),escape(&post.description)));
        if let Some(date) = rss_date(&post.date) {
            out.push_str(&format!("<pubDate>{date}</pubDate>"));
        }
        for author in &post.authors {
            out.push_str(&format!(
                "<dc:creator>{}</dc:creator>",
                escape(&author.name)
            ));
        }
        let cover = post
            .cover
            .clone()
            .unwrap_or_else(|| format!("{origin}/open-graph/default.jpg"));
        out.push_str(&format!(
            "<media:content url=\"{}\" medium=\"image\" />",
            escape(&cover)
        ));
        out.push_str("</item>");
    }
    out.push_str("</channel></rss>");
    out
}
fn sitemap(posts: &[Post], origin: &str) -> String {
    let mut paths: Vec<String> = ROUTES.iter().map(|(p, _)| (*p).into()).collect();
    paths.extend(
        releases()
            .iter()
            .map(|r| format!("/changelog/{}", r.version)),
    );
    paths.extend(posts.iter().map(|p| format!("/blog/{}", p.slug)));
    let mut out="<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">".to_owned();
    for path in paths {
        out.push_str(&format!(
            "<url><loc>{}</loc></url>",
            escape(&format!("{origin}{path}"))
        ));
    }
    out.push_str("</urlset>");
    out
}
#[cfg(test)]
mod tests;
