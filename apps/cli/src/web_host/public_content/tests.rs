use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn post_value(slug: &str) -> Value {
    json!({"id":slug,"slug":slug,"title":"Source title","description":"Source description","content":"<p>Body</p>","publishedAt":"2026-10-16T08:00:00Z","authors":[]})
}
fn page(slug: &str, number: u32, total: u32) -> Value {
    json!({"posts":[post_value(slug)],"pagination":{"currpage":number,"totalPages":total,"nextPage":if number<total {Some(number+1)} else {None}}})
}

#[test]
fn source_projection_rejects_mismatched_identity_and_limits_before_publication() {
    let key = Key::Post("safe".into());
    assert!(matches!(
        parse(&key, br#"{"post":null}"#),
        Err(Failure::NotFound)
    ));
    assert!(matches!(
        parse(
            &key,
            &serde_json::to_vec(&json!({"post":post_value("other")})).unwrap()
        ),
        Err(Failure::Invalid)
    ));
    assert!(matches!(
        parse(&key, &vec![b' '; MAX_BYTES + 1]),
        Err(Failure::Invalid)
    ));
    let mut value = post_value("safe");
    value["coverImage"] = json!("https://user:password@example.com/image.jpg");
    value["authors"] = json!([{"name":"Author","image":"javascript:alert(1)"}]);
    let Content::Post(post) =
        parse(&key, &serde_json::to_vec(&json!({"post":value})).unwrap()).unwrap()
    else {
        panic!("post")
    };
    assert!(post.cover.is_none() && post.authors[0].image.is_none());
    for key in [
        Key::Page(0),
        Key::Page(MAX_PAGES + 1),
        Key::Post("../token".into()),
        Key::Post("x?cookie=secret".into()),
        Key::Post("x%2fy".into()),
    ] {
        assert!(!key.valid())
    }
    assert_eq!(
        Key::Post("source-1".into()).url(),
        format!("{MARBLE}/posts/source-1")
    );
    let mut invalid = page("one", 1, 2);
    invalid["pagination"]["nextPage"] = json!(1);
    assert!(parse(&Key::Page(1), &serde_json::to_vec(&invalid).unwrap()).is_err());
    invalid["pagination"]["totalPages"] = json!("two");
    assert!(parse(&Key::Page(1), &serde_json::to_vec(&invalid).unwrap()).is_err());
    let empty = json!({"posts":[],"pagination":{"currpage":1,"totalPages":0,"nextPage":null}});
    assert!(parse(&Key::Page(1), &serde_json::to_vec(&empty).unwrap()).is_ok());
}

#[tokio::test]
async fn sitemap_reads_all_bounded_pages_without_retaining_rich_text_or_partial_results() {
    let content = PublicContent::fixture(vec![
        (Key::Page(1), Ok(page("one", 1, 2))),
        (Key::Page(2), Ok(page("two", 2, 2))),
    ]);
    let posts = content.all_posts().await.unwrap();
    assert_eq!(
        posts.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
        ["one", "two"]
    );
    assert!(posts.iter().all(|p| p.content.is_empty()));
    let failed = PublicContent::fixture(vec![
        (Key::Page(1), Ok(page("one", 1, 2))),
        (Key::Page(2), Err(Failure::Unavailable)),
    ]);
    assert!(matches!(
        failed.all_posts().await,
        Err(Failure::Unavailable)
    ));
    let duplicate = PublicContent::fixture(vec![
        (Key::Page(1), Ok(page("one", 1, 2))),
        (Key::Page(2), Ok(page("one", 2, 2))),
    ]);
    assert!(matches!(duplicate.all_posts().await, Err(Failure::Invalid)));
    let changed = PublicContent::fixture(vec![
        (Key::Page(1), Ok(page("one", 1, 2))),
        (Key::Page(2), Ok(page("two", 2, 3))),
    ]);
    assert!(matches!(changed.all_posts().await, Err(Failure::Invalid)));
}

#[tokio::test]
async fn public_cache_expires_bounds_entries_and_never_calls_source_for_invalid_selectors() {
    struct Counting(Arc<AtomicUsize>);
    impl Source for Counting {
        fn read(&self, key: &Key) -> Result<Vec<u8>, Failure> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let Key::Post(slug) = key else {
                return Err(Failure::Invalid);
            };
            if slug == "absent" {
                return Err(Failure::NotFound);
            }
            Ok(serde_json::to_vec(&json!({"post":post_value(slug)})).unwrap())
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let content = PublicContent {
        source: Arc::new(Counting(calls.clone())),
        ..PublicContent::new()
    };
    assert!(content.get(Key::Post("a/b".into())).await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let key = Key::Post("safe".into());
    content.get(key.clone()).await.unwrap();
    content.get(key.clone()).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    content.cache.lock().unwrap().get_mut(&key).unwrap().expires = Instant::now();
    content.get(key).await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    for _ in 0..2 {
        assert!(matches!(
            content.get(Key::Post("absent".into())).await,
            Err(Failure::NotFound)
        ))
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    for i in 0..70 {
        content.get(Key::Post(format!("post-{i}"))).await.unwrap();
    }
    let cache = content.cache.lock().unwrap();
    assert!(cache.len() <= 64);
    assert!(cache.values().map(|v| v.bytes).sum::<usize>() <= CACHE_BYTES);
}
