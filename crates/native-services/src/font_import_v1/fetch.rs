//! Fixed-source public font retrieval. Explicit selection retains host proxy settings;
//! no project text, application credentials or user URLs cross this boundary.
use opencut_contracts::EditorError;
use opencut_editor_application::font_import_v1::{catalog::*, *};
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    time::{Duration, Instant},
};

pub struct GoogleFontsFetchV1;

fn unavailable() -> EditorError {
    EditorError::new(
        "FONT_SOURCE_UNAVAILABLE",
        "Pinned public font could not be downloaded",
    )
}
fn integrity() -> EditorError {
    EditorError::new(
        "FONT_SOURCE_INTEGRITY_ERROR",
        "Font download does not match the pinned source",
    )
}
fn timeout() -> EditorError {
    EditorError::new(
        "FONT_SOURCE_TIMEOUT",
        "Pinned font download deadline exceeded",
    )
}
fn check(
    deadline: Instant,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<(), EditorError> {
    checkpoint.check()?;
    if Instant::now() >= deadline {
        return Err(timeout());
    }
    Ok(())
}

/// The fixed HTTPS source prefers its scheme-specific proxy. Ureq's default
/// ALL_PROXY precedence can select SOCKS without a compiled SOCKS transport.
/// Explicit configuration prevents that transport from silently bypassing it.
pub(crate) fn source_proxy(
    source_url: &str,
    mut environment: impl FnMut(&str) -> Option<String>,
) -> Result<Option<ureq::Proxy>, EditorError> {
    let uri = source_url.parse().map_err(|_| unavailable())?;
    // Match ureq 3.2's uppercase precedence and comma expressions exactly,
    // including empty expressions; use its matcher rather than duplicating it.
    let no_proxy = environment("NO_PROXY").or_else(|| environment("no_proxy"));
    let mut unsupported = false;
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        let Some(value) = environment(key) else {
            continue;
        };
        let parsed = ureq::Proxy::new(&value).map_err(|_| proxy_error())?;
        let mut builder = ureq::Proxy::builder(parsed.protocol())
            .host(parsed.host())
            .port(parsed.port());
        if let Some(value) = parsed.username() {
            builder = builder.username(value);
        }
        if let Some(value) = parsed.password() {
            builder = builder.password(value);
        }
        if let Some(expressions) = &no_proxy {
            for expression in expressions.split(',') {
                builder = builder.no_proxy(expression);
            }
        }
        let proxy = builder.build().map_err(|_| proxy_error())?;
        if proxy.is_no_proxy(&uri) {
            return Ok(None);
        }
        if matches!(
            proxy.protocol(),
            ureq::ProxyProtocol::Http | ureq::ProxyProtocol::Https
        ) {
            return Ok(Some(proxy));
        }
        unsupported = true;
    }
    if unsupported {
        Err(proxy_error())
    } else {
        Ok(None)
    }
}
fn proxy_error() -> EditorError {
    EditorError::new(
        "FONT_PROXY_UNAVAILABLE",
        "Font download proxy is invalid or requires unavailable SOCKS support",
    )
}

pub(crate) fn read_pinned_bytes(
    mut reader: impl Read,
    size: u64,
    hash: &str,
    deadline: Instant,
    checkpoint: &mut dyn FontImportCheckpointV1,
) -> Result<Vec<u8>, EditorError> {
    let mut result = Vec::with_capacity(size as usize);
    let mut digest = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        check(deadline, checkpoint)?;
        let read = reader.read(&mut buffer);
        // A revoked scope/cancel signal retains its original error even when
        // the pending read also fails. Blocking I/O remains bounded by ureq's
        // global deadline, not by an assertion of instantaneous cancellation.
        check(deadline, checkpoint)?;
        let count = read.map_err(|_| unavailable())?;
        if count == 0 {
            break;
        }
        if result.len() as u64 + count as u64 > size {
            return Err(integrity());
        }
        result.extend_from_slice(&buffer[..count]);
        digest.update(&buffer[..count]);
    }
    if result.len() as u64 != size || format!("{:x}", digest.finalize()) != hash {
        return Err(integrity());
    }
    check(deadline, checkpoint)?;
    Ok(result)
}

impl FontSourceFetchV1 for GoogleFontsFetchV1 {
    fn fetch(
        &self,
        catalog_id: &str,
        policy: &FontFetchPolicyV1,
        checkpoint: &mut dyn FontImportCheckpointV1,
    ) -> Result<Vec<u8>, EditorError> {
        checkpoint.check()?;
        let source = validate_font_fetch_v1(catalog_id, policy)?;
        let duration = Duration::from_millis(policy.deadline_ms);
        let deadline = Instant::now() + duration;
        // No redirects, cookie jar, arbitrary request headers or gzip. TLS and
        // environment-configured HTTP(S) proxies use the existing native stack.
        let proxy = source_proxy(source.source_url, |key| std::env::var(key).ok())?;
        let agent = ureq::Agent::config_builder()
            .proxy(proxy)
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(duration))
            .timeout_resolve(Some(duration.min(Duration::from_secs(5))))
            .timeout_connect(Some(duration.min(Duration::from_secs(5))))
            .timeout_recv_response(Some(duration.min(Duration::from_secs(10))))
            .build()
            .new_agent();
        check(deadline, checkpoint)?;
        let response = agent
            .get(source.source_url)
            .header("Accept", "font/ttf,application/octet-stream")
            .header("Accept-Encoding", "identity")
            .header("User-Agent", "OpenCut-Font-Import-V1")
            .call();
        check(deadline, checkpoint)?;
        let mut response = response.map_err(|error| match error {
            ureq::Error::Timeout(_) => timeout(),
            _ => unavailable(),
        })?;
        if response.status().as_u16() != 200 {
            return Err(unavailable());
        }
        if response
            .headers()
            .get("content-encoding")
            .is_some_and(|value| value.as_bytes() != b"identity")
        {
            return Err(integrity());
        }
        if let Some(value) = response.headers().get("content-length") {
            let length = value
                .to_str()
                .ok()
                .and_then(|text| text.parse::<u64>().ok());
            if length != Some(source.size_bytes) {
                return Err(integrity());
            }
        }
        let reader = response
            .body_mut()
            .with_config()
            .limit(source.size_bytes + 1)
            .reader();
        read_pinned_bytes(
            reader,
            source.size_bytes,
            source.content_hash,
            deadline,
            checkpoint,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proxy_for(host: &str, vars: &[(&str, &str)]) -> Result<Option<ureq::Proxy>, EditorError> {
        source_proxy(&format!("https://{host}/font.ttf"), |key| {
            vars.iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).into())
        })
    }
    #[test]
    fn https_proxy_wins_over_socks_and_preserves_proxy_authentication() {
        let proxy = proxy_for(
            "raw.githubusercontent.com",
            &[
                ("HTTPS_PROXY", "http://user:secret@127.0.0.1:7890"),
                ("https_proxy", "http://127.0.0.1:7891"),
                ("ALL_PROXY", "socks5://127.0.0.1:7892"),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(proxy.protocol(), ureq::ProxyProtocol::Http);
        assert_eq!(proxy.port(), 7890);
        assert_eq!(proxy.username(), Some("user"));
        assert_eq!(proxy.password(), Some("secret"));
        let proxy = proxy_for(
            "raw.githubusercontent.com",
            &[
                ("https_proxy", "http://127.0.0.1:7891"),
                ("ALL_PROXY", "socks5://127.0.0.1:7892"),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(proxy.port(), 7891);
    }
    #[test]
    fn unsupported_socks_never_falls_back_to_direct_transport() {
        assert_eq!(
            proxy_for(
                "raw.githubusercontent.com",
                &[("ALL_PROXY", "socks5://127.0.0.1:7890"),]
            )
            .unwrap_err()
            .code,
            "FONT_PROXY_UNAVAILABLE"
        );
        let proxy = proxy_for(
            "raw.githubusercontent.com",
            &[
                ("ALL_PROXY", "socks5://127.0.0.1:7890"),
                ("HTTP_PROXY", "http://127.0.0.1:7891"),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(proxy.port(), 7891);
        assert!(
            proxy_for("raw.githubusercontent.com", &[])
                .unwrap()
                .is_none()
        );
        assert_eq!(
            proxy_for(
                "raw.githubusercontent.com",
                &[("HTTPS_PROXY", "invalid://localhost"),]
            )
            .unwrap_err()
            .code,
            "FONT_PROXY_UNAVAILABLE"
        );
    }
    #[test]
    fn no_proxy_uses_ureq_matcher_and_uppercase_precedence() {
        for expression in [
            "raw.githubusercontent.com",
            "*.githubusercontent.com",
            ".githubusercontent.com",
            "*",
        ] {
            assert!(
                proxy_for(
                    "raw.githubusercontent.com",
                    &[
                        ("ALL_PROXY", "socks5://127.0.0.1:7890"),
                        ("NO_PROXY", expression),
                    ]
                )
                .unwrap()
                .is_none()
            );
        }
        assert_eq!(
            proxy_for(
                "githubusercontent.com",
                &[
                    ("ALL_PROXY", "socks5://127.0.0.1:7890"),
                    ("NO_PROXY", ".githubusercontent.com"),
                ]
            )
            .unwrap_err()
            .code,
            "FONT_PROXY_UNAVAILABLE"
        );
        assert!(
            proxy_for(
                "raw.githubusercontent.com",
                &[
                    ("HTTPS_PROXY", "http://127.0.0.1:7890"),
                    ("NO_PROXY", "localhost"),
                    ("no_proxy", "*"),
                ]
            )
            .unwrap()
            .is_some()
        );
        assert!(
            proxy_for(
                "RAW.GITHUBUSERCONTENT.COM",
                &[
                    ("HTTPS_PROXY", "http://127.0.0.1:7890"),
                    ("no_proxy", "localhost,raw.githubusercontent.com"),
                ]
            )
            .unwrap()
            .is_none()
        );
    }
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    #[test]
    fn exact_bytes_survive_but_truncated_mutated_and_overlong_bodies_do_not() {
        let until = Instant::now() + Duration::from_secs(1);
        assert_eq!(
            read_pinned_bytes(&b"abc"[..], 3, ABC, until, &mut || Ok(())).unwrap(),
            b"abc"
        );
        for bytes in [&b"ab"[..], &b"abd"[..], &b"abcd"[..]] {
            assert_eq!(
                read_pinned_bytes(bytes, 3, ABC, until, &mut || Ok(()))
                    .unwrap_err()
                    .code,
                "FONT_SOURCE_INTEGRITY_ERROR"
            );
        }
    }
    #[test]
    fn policy_and_catalog_are_checked_before_constructing_network_requests() {
        assert_eq!(
            GoogleFontsFetchV1
                .fetch(
                    "google-lato-regular-v1",
                    &FontFetchPolicyV1::default(),
                    &mut || Ok(())
                )
                .unwrap_err()
                .code,
            "FONT_NETWORK_DENIED"
        );
        let policy = FontFetchPolicyV1 {
            allow_public_font_download: true,
            ..Default::default()
        };
        assert_eq!(
            GoogleFontsFetchV1
                .fetch("https://example.com/font", &policy, &mut || Ok(()))
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
        assert_eq!(
            GoogleFontsFetchV1
                .fetch("google-lato-regular-v1", &policy, &mut || Err(
                    EditorError::new("PERMISSION_DENIED", "revoked")
                ))
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
    }
    #[test]
    fn cancellation_after_a_read_and_expired_deadline_return_no_bytes() {
        let until = Instant::now() + Duration::from_secs(1);
        let mut calls = 0;
        let error = read_pinned_bytes(&b"abc"[..], 3, ABC, until, &mut || {
            calls += 1;
            if calls == 2 {
                Err(EditorError::new("JOB_CANCELLED", "cancel after bytes"))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.code, "JOB_CANCELLED");
        assert_eq!(
            read_pinned_bytes(&b"abc"[..], 3, ABC, Instant::now(), &mut || Ok(()))
                .unwrap_err()
                .code,
            "FONT_SOURCE_TIMEOUT"
        );
    }
}
