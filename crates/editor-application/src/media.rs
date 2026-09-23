//! Media identity and metadata policy, independent of files and codecs.
pub use opencut_api_contracts::media::{InspectMediaInput, RepairMediaInput};
use opencut_contracts::{Asset, AssetKind, CatalogEntry, EditorError, Time};
use opencut_media_contracts::{MediaMetadata, StreamKind};
use serde_json::json;
use std::collections::BTreeMap;

pub struct DetectedMedia {
    pub metadata: MediaMetadata,
    pub uri: String,
    pub name: String,
    pub content_hash: String,
    pub size: u64,
}
/// Repair must compare expected metadata under the same per-content lock used
/// by import, verify and probe a private copy of the exact content, run classify,
/// and atomically publish its result. Any failure preserves previous metadata.
pub trait MediaCatalog {
    fn inspect(&self, hash: &str) -> Result<CatalogEntry, EditorError>;
    fn repair(
        &self,
        input: &RepairMediaInput,
        classify: &dyn Fn(DetectedMedia) -> Result<Asset, EditorError>,
    ) -> Result<CatalogEntry, EditorError>;
    fn publish(&self, hash: &str, asset: &Asset) -> Result<(), EditorError>;
}
fn identity(hash: &str) -> Result<(), EditorError> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Expected a lowercase SHA-256 media identity",
        ));
    }
    Ok(())
}
pub fn inspect(
    store: &impl MediaCatalog,
    input: InspectMediaInput,
) -> Result<CatalogEntry, EditorError> {
    identity(&input.content_hash)?;
    store.inspect(&input.content_hash)
}
pub fn repair(
    store: &impl MediaCatalog,
    input: RepairMediaInput,
) -> Result<CatalogEntry, EditorError> {
    identity(&input.content_hash)?;
    store.repair(&input, &|detected| {
        if detected.content_hash != input.content_hash {
            return Err(EditorError::new(
                "INTERNAL_ERROR",
                "Media observation belongs to another content identity",
            ));
        }
        describe(
            &detected.metadata,
            &detected.uri,
            &detected.name,
            &detected.content_hash,
            detected.size,
        )
    })
}
pub fn publish(store: &impl MediaCatalog, hash: &str, asset: &Asset) -> Result<(), EditorError> {
    identity(hash)?;
    store.publish(hash, asset)
}

pub fn describe<P>(
    metadata: &MediaMetadata<P>,
    uri: &str,
    name: &str,
    hash: &str,
    size: u64,
) -> Result<Asset, EditorError> {
    let video = metadata
        .streams
        .iter()
        .find(|s| s.codec_type == StreamKind::Video);
    let audio = metadata
        .streams
        .iter()
        .any(|s| s.codec_type == StreamKind::Audio);
    let format = metadata.format_name.as_deref().unwrap_or("");
    let video_codec = video.and_then(|stream| stream.codec_name.as_deref());
    // A PNG/MJPEG codec inside a movie is still a video. Classify the
    // container detected from bytes, never the supplied filename or codec alone.
    let image = video.is_some()
        && format.split(',').any(|part| {
            matches!(
                part,
                "image2"
                    | "image2pipe"
                    | "png_pipe"
                    | "jpeg_pipe"
                    | "webp_pipe"
                    | "bmp_pipe"
                    | "tiff_pipe"
            )
        });
    let (kind, mime) = if image {
        (
            AssetKind::Image,
            match video_codec {
                Some("png") => "image/png",
                Some("webp") => "image/webp",
                Some("bmp") => "image/bmp",
                Some("tiff") => "image/tiff",
                _ => "image/jpeg",
            },
        )
    } else if video.is_some() {
        (
            AssetKind::Video,
            if format.split(',').any(|part| part == "matroska") {
                // ffprobe uses a shared Matroska/WebM demuxer. Do not assert
                // WebM codec restrictions from that ambiguous format name.
                "application/octet-stream"
            } else if format == "avi" {
                "video/x-msvideo"
            } else if format == "gif" {
                "image/gif"
            } else if format.split(',').any(|part| part == "mov")
                && !format.split(',').any(|part| part == "mp4")
            {
                "video/quicktime"
            } else if format.split(',').any(|part| part == "mp4") {
                "video/mp4"
            } else {
                "application/octet-stream"
            },
        )
    } else if audio {
        (
            AssetKind::Audio,
            if format.split(',').any(|part| part == "wav") {
                "audio/wav"
            } else if format.split(',').any(|part| part == "mp3") {
                "audio/mpeg"
            } else if format.split(',').any(|part| part == "ogg") {
                "audio/ogg"
            } else if format == "flac" {
                "audio/flac"
            } else if format.split(',').any(|part| part == "mp4") {
                "audio/mp4"
            } else {
                "application/octet-stream"
            },
        )
    } else {
        return Err(EditorError::new(
            "CAPABILITY_UNAVAILABLE",
            "Upload contains no supported visual or audio stream",
        ));
    };
    let mut fields = BTreeMap::new();
    fields.insert("content_hash".into(), json!(hash));
    fields.insert("media_url".into(), json!(format!("/v1/media/{hash}")));
    fields.insert("mime_type".into(), json!(mime));
    fields.insert("size".into(), json!(size));
    if let Some(video) = video {
        fields.insert("width".into(), json!(video.width));
        fields.insert("height".into(), json!(video.height));
    }
    fields.insert("has_audio".into(), json!(audio));
    let asset = Asset {
        id: format!("asset_{hash}"),
        name: name.into(),
        kind,
        uri: uri.into(),
        duration: if image {
            None
        } else {
            metadata.duration.map(|t| Time {
                ticks: t.ticks.to_string(),
                timebase: t.timebase,
            })
        },
        version: None,
        metadata: fields,
    };
    Ok(asset)
}
