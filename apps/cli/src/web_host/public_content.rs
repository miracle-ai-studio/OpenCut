//! Bounded adapter for the same fixed public Marble source used by Classic.
//! No project data, application credentials or request-selected URLs are sent.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::Read,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const MARBLE: &str = "https://api.marblecms.com/cmd4iw9mm0006l804kwqv0k46";
const MAX_BYTES: usize = 4 * 1024 * 1024;
const CACHE_BYTES: usize = 8 * 1024 * 1024;
const MAX_PAGES: u32 = 20;
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum Key {
    Page(u32),
    Post(String),
}
impl Key {
    fn url(&self) -> String {
        match self {
            Self::Page(p) => format!("{MARBLE}/posts?page={p}"),
            Self::Post(s) => format!("{MARBLE}/posts/{s}"),
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::Page(p) => (1..=MAX_PAGES).contains(p),
            Self::Post(s) => slug(s),
        }
    }
}
pub(super) fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
#[derive(Clone, Debug)]
pub(super) enum Failure {
    NotFound,
    Unavailable,
    Invalid,
    Busy,
}
impl Failure {
    pub fn status(&self) -> axum::http::StatusCode {
        use axum::http::StatusCode as S;
        match self {
            Self::NotFound => S::NOT_FOUND,
            Self::Busy => S::SERVICE_UNAVAILABLE,
            Self::Invalid | Self::Unavailable => S::BAD_GATEWAY,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Author {
    pub name: String,
    pub image: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Post {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub content: String,
    #[serde(rename = "coverImage")]
    pub cover: Option<String>,
    #[serde(rename = "publishedAt")]
    pub date: String,
    pub authors: Vec<Author>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Pagination {
    pub currpage: u32,
    #[serde(rename = "totalPages")]
    pub total: u32,
    #[serde(rename = "nextPage")]
    pub next: Option<u32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Page {
    pub posts: Vec<Post>,
    pub pagination: Pagination,
}
#[derive(Clone, Debug)]
pub(super) enum Content {
    Page(Page),
    Post(Post),
}
impl Content {
    pub fn json(&self) -> Value {
        match self {
            Self::Page(p) => serde_json::to_value(p).expect("public page"),
            Self::Post(p) => serde_json::json!({"post":p}),
        }
    }
}
fn text(v: Option<&Value>, max: usize) -> String {
    v.and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(max)
        .collect()
}
fn public_url(v: Option<&Value>) -> Option<String> {
    let value = v?.as_str()?;
    let uri: axum::http::Uri = value.parse().ok()?;
    (value.len() <= 2048
        && uri.scheme_str() == Some("https")
        && uri.authority().is_some_and(|a| !a.as_str().contains('@')))
    .then(|| value.into())
}
fn post(v: &Value) -> Result<Post, Failure> {
    let name = text(v.get("slug"), 201);
    let title = text(v.get("title"), 300);
    if !slug(&name) || title.trim().is_empty() {
        return Err(Failure::Invalid);
    }
    Ok(Post {
        id: text(v.get("id"), 1000),
        slug: name,
        title,
        description: text(v.get("description"), 3000),
        content: text(v.get("content"), 500000),
        cover: public_url(v.get("coverImage")),
        date: text(v.get("publishedAt"), 100),
        authors: v
            .get("authors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(10)
            .filter_map(|a| {
                let name = text(a.get("name"), 100);
                (!name.trim().is_empty()).then(|| Author {
                    name,
                    image: public_url(a.get("image")),
                })
            })
            .collect(),
    })
}
fn parse(key: &Key, bytes: &[u8]) -> Result<Content, Failure> {
    if bytes.len() > MAX_BYTES {
        return Err(Failure::Invalid);
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| Failure::Invalid)?;
    match key {
        Key::Post(expected) => {
            let value = v
                .get("post")
                .filter(|p| !p.is_null())
                .ok_or(Failure::NotFound)?;
            let p = post(value)?;
            if &p.slug != expected {
                return Err(Failure::Invalid);
            }
            Ok(Content::Post(p))
        }
        Key::Page(expected) => {
            let values = v
                .get("posts")
                .and_then(Value::as_array)
                .ok_or(Failure::Invalid)?;
            if values.len() > 50 {
                return Err(Failure::Invalid);
            }
            let posts = values.iter().map(post).collect::<Result<Vec<_>, _>>()?;
            let p = v.get("pagination");
            if p.is_some_and(|p| !p.is_object()) {
                return Err(Failure::Invalid);
            }
            let number = |name: &str| -> Result<Option<u64>, Failure> {
                p.and_then(|p| p.get(name))
                    .filter(|value| !value.is_null())
                    .map(|value| value.as_u64().ok_or(Failure::Invalid))
                    .transpose()
            };
            let current = number("currpage")?.unwrap_or(1);
            let total = number("totalPages")?.unwrap_or(1);
            // Classic explicitly represents an empty public feed as zero pages.
            if total == 0 && !posts.is_empty() {
                return Err(Failure::Invalid);
            }
            let total = total.max(1);
            if current != u64::from(*expected) || total > u64::from(MAX_PAGES) || current > total {
                return Err(Failure::Invalid);
            }
            let next = number("nextPage")?;
            if next.is_some_and(|next| next <= current || next > total) {
                return Err(Failure::Invalid);
            }
            Ok(Content::Page(Page {
                posts,
                pagination: Pagination {
                    currpage: *expected,
                    total: total as u32,
                    next: next.map(|v| v as u32),
                },
            }))
        }
    }
}
trait Source: Send + Sync {
    fn read(&self, key: &Key) -> Result<Vec<u8>, Failure>;
}
struct Network;
// Prefer the scheme-specific configured proxy, preserving NO_PROXY. Unsupported
// proxy transports fail explicitly; this adapter never silently bypasses one.
fn proxy(url: &str) -> Result<Option<ureq::Proxy>, Failure> {
    let uri = url.parse().map_err(|_| Failure::Unavailable)?;
    let no_proxy = std::env::var("NO_PROXY")
        .or_else(|_| std::env::var("no_proxy"))
        .ok();
    let mut unsupported = false;
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        let p = ureq::Proxy::new(&value).map_err(|_| Failure::Unavailable)?;
        let mut b = ureq::Proxy::builder(p.protocol())
            .host(p.host())
            .port(p.port());
        if let Some(value) = p.username() {
            b = b.username(value)
        }
        if let Some(value) = p.password() {
            b = b.password(value)
        }
        if let Some(value) = &no_proxy {
            for expression in value.split(',') {
                b = b.no_proxy(expression)
            }
        }
        let p = b.build().map_err(|_| Failure::Unavailable)?;
        if p.is_no_proxy(&uri) {
            return Ok(None);
        }
        if matches!(
            p.protocol(),
            ureq::ProxyProtocol::Http | ureq::ProxyProtocol::Https
        ) {
            return Ok(Some(p));
        }
        unsupported = true;
    }
    if unsupported {
        Err(Failure::Unavailable)
    } else {
        Ok(None)
    }
}
impl Source for Network {
    fn read(&self, key: &Key) -> Result<Vec<u8>, Failure> {
        let url = key.url();
        let config = ureq::Agent::config_builder()
            .proxy(proxy(&url)?)
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(15)))
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_resolve(Some(Duration::from_secs(5)))
            .build();
        let mut response = ureq::Agent::new_with_config(config)
            .get(&url)
            .header("accept", "application/json")
            .call()
            .map_err(|_| Failure::Unavailable)?;
        if response.status().as_u16() == 404 {
            return Err(Failure::NotFound);
        }
        if !response.status().is_success() {
            return Err(Failure::Unavailable);
        }
        let mut bytes = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| Failure::Unavailable)?;
        if bytes.len() > MAX_BYTES {
            return Err(Failure::Invalid);
        }
        Ok(bytes)
    }
}
struct Cached {
    value: Result<Arc<Content>, Failure>,
    expires: Instant,
    inserted: Instant,
    bytes: usize,
}
#[derive(Clone)]
pub(super) struct PublicContent {
    source: Arc<dyn Source>,
    cache: Arc<Mutex<HashMap<Key, Cached>>>,
    slots: Arc<tokio::sync::Semaphore>,
}
impl PublicContent {
    pub fn new() -> Self {
        Self {
            source: Arc::new(Network),
            cache: Arc::new(Mutex::new(HashMap::new())),
            slots: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }
    fn cached(&self, key: &Key) -> Option<Result<Arc<Content>, Failure>> {
        let mut cache = self.cache.lock().ok()?;
        cache.retain(|_, v| v.expires > Instant::now());
        cache.get(key).map(|v| v.value.clone())
    }
    pub async fn get(&self, key: Key) -> Result<Arc<Content>, Failure> {
        if !key.valid() {
            return Err(Failure::NotFound);
        }
        if let Some(v) = self.cached(&key) {
            return v;
        }
        let slot = tokio::time::timeout(Duration::from_secs(1), self.slots.clone().acquire_owned())
            .await
            .map_err(|_| Failure::Busy)?
            .map_err(|_| Failure::Busy)?;
        if let Some(v) = self.cached(&key) {
            return v;
        }
        let source = self.source.clone();
        let selected = key.clone();
        let value = tokio::task::spawn_blocking(move || {
            let _slot = slot;
            source
                .read(&selected)
                .and_then(|b| parse(&selected, &b))
                .map(Arc::new)
        })
        .await
        .map_err(|_| Failure::Unavailable)?;
        let bytes = value.as_ref().map_or(0, |v| {
            serde_json::to_vec(&v.json()).map_or(MAX_BYTES + 1, |v| v.len())
        });
        if bytes <= MAX_BYTES {
            if let Ok(mut cache) = self.cache.lock() {
                let now = Instant::now();
                cache.retain(|_, v| v.expires > now);
                while cache.len() >= 64
                    || cache.values().map(|v| v.bytes).sum::<usize>() + bytes > CACHE_BYTES
                {
                    let oldest = cache
                        .iter()
                        .min_by_key(|(_, v)| v.inserted)
                        .map(|(k, _)| k.clone());
                    if let Some(k) = oldest {
                        cache.remove(&k);
                    } else {
                        break;
                    }
                }
                cache.insert(
                    key,
                    Cached {
                        value: value.clone(),
                        expires: now + Duration::from_secs(if value.is_ok() { 60 } else { 15 }),
                        inserted: now,
                        bytes,
                    },
                );
            }
        }
        value
    }
    pub async fn all_posts(&self) -> Result<Vec<Post>, Failure> {
        let first = self.get(Key::Page(1)).await?;
        let Content::Page(first) = first.as_ref() else {
            return Err(Failure::Invalid);
        };
        // Sitemap needs metadata, never the potentially large rich-text bodies.
        let metadata = |p: &Post| Post {
            id: p.id.clone(),
            slug: p.slug.clone(),
            title: p.title.clone(),
            description: p.description.clone(),
            content: String::new(),
            cover: p.cover.clone(),
            date: p.date.clone(),
            authors: p.authors.clone(),
        };
        let mut posts: Vec<_> = first.posts.iter().map(metadata).collect();
        for n in 2..=first.pagination.total {
            let page = self.get(Key::Page(n)).await?;
            let Content::Page(page) = page.as_ref() else {
                return Err(Failure::Invalid);
            };
            if page.pagination.total != first.pagination.total {
                return Err(Failure::Invalid);
            }
            posts.extend(page.posts.iter().map(metadata));
        }
        let mut seen = std::collections::HashSet::new();
        if posts.iter().any(|p| !seen.insert(p.slug.clone())) {
            return Err(Failure::Invalid);
        }
        Ok(posts)
    }
    #[cfg(test)]
    pub(super) fn fixture(entries: Vec<(Key, Result<Value, Failure>)>) -> Self {
        struct Fixture(HashMap<Key, Result<Value, Failure>>);
        impl Source for Fixture {
            fn read(&self, key: &Key) -> Result<Vec<u8>, Failure> {
                self.0
                    .get(key)
                    .cloned()
                    .unwrap_or(Err(Failure::NotFound))
                    .map(|value| serde_json::to_vec(&value).unwrap())
            }
        }
        Self {
            source: Arc::new(Fixture(entries.into_iter().collect())),
            ..Self::new()
        }
    }
}
#[cfg(test)]
mod tests;
