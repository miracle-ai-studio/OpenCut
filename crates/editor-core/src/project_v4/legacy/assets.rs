//! Typed observation projection; unfamiliar metadata is never silently dropped.
use super::*;

pub(super) fn convert_assets(
    base: &ProjectDocument,
    inputs: &LegacyConversionInputs,
    context: &mut Context,
) -> Vec<AssetV4> {
    for id in inputs.asset_versions.keys() {
        if !base.assets.iter().any(|asset| &asset.id == id) {
            context.issue(
                format!("asset_versions[{id}]"),
                LegacyConversionIssueKind::UnknownVersionBinding {
                    asset_id: id.clone(),
                },
            );
        }
    }
    let mut result = Vec::new();
    for (index, asset) in base.assets.iter().enumerate() {
        let path = format!("assets[{index}]");
        let supplied = inputs.asset_versions.get(&asset.id);
        if let (Some(old), Some(binding)) = (&asset.version, supplied)
            && old != binding
        {
            context.issue(
                format!("{path}.version"),
                LegacyConversionIssueKind::ConflictingAssetVersion {
                    asset_id: asset.id.clone(),
                },
            );
        }
        let version = asset.version.as_ref().or(supplied);
        if let Some(version) = version {
            if let Err(error) = crate::validate_asset_version(version) {
                context.issue(
                    format!("{path}.version"),
                    LegacyConversionIssueKind::InvalidAssetVersion {
                        asset_id: asset.id.clone(),
                        message: error.message,
                    },
                );
            }
            context
                .report
                .resource_bindings
                .push(LegacyResourceBinding {
                    asset_id: asset.id.clone(),
                    version: version.clone(),
                    supplied: asset.version.is_none(),
                });
        } else {
            context.issue(
                format!("{path}.version"),
                LegacyConversionIssueKind::MissingAssetVersion {
                    asset_id: asset.id.clone(),
                },
            );
        }
        // Inspect all fields even when a version is missing, so callers receive
        // one complete actionable report rather than a sequence of surprises.
        let metadata = metadata(asset, version, &path, context);
        if let Some(version) = version {
            result.push(AssetV4 {
                id: asset.id.clone(),
                name: asset.name.clone(),
                kind: asset.kind.clone(),
                version: version.clone(),
                duration: asset.duration.clone(),
                origin: ResourceOriginV4::LegacyLocator {
                    uri: asset.uri.clone(),
                },
                metadata,
                license: None,
            });
        }
    }
    result
}

fn metadata(
    asset: &Asset,
    version: Option<&AssetVersion>,
    path: &str,
    context: &mut Context,
) -> MediaMetadataV4 {
    let mut result = MediaMetadataV4 {
        intrinsic_size: None,
        frame_rate: None,
        rotation_degrees: None,
        audio: AudioPresenceV4::Unknown {},
        mime_type: None,
    };
    let mut width = None;
    let mut height = None;
    let mut audio_metadata = None;
    for (field, value) in &asset.metadata {
        let location = format!("{path}.metadata.{field}");
        let mut bad = |reason: &str| {
            context.issue(
                location.clone(),
                LegacyConversionIssueKind::InvalidMetadata {
                    field: field.clone(),
                    reason: reason.into(),
                },
            )
        };
        match field.as_str() {
            "audio_metadata" => match serde_json::from_value::<AudioMetadataV4>(value.clone()) {
                Ok(metadata) if metadata.sample_rate > 0 && metadata.channels > 0 => {
                    audio_metadata = Some(metadata);
                }
                _ => {
                    bad("requires typed audio metadata with positive u32 sample rate and channels")
                }
            },
            "content_hash" => match (value.as_str(), version) {
                (Some(hash), Some(version)) if hash == version.content_hash => context
                    .report
                    .normalizations
                    .push(LegacyNormalization::MetadataIdentityCanonicalized {
                        asset_id: asset.id.clone(),
                        field: field.clone(),
                    }),
                (Some(_), None) => {} // MissingAssetVersion already identifies the required authority.
                _ => bad("must equal the immutable resource content hash"),
            },
            "size" => match (value.as_u64(), version) {
                (Some(size), Some(version)) if size == version.size => context
                    .report
                    .normalizations
                    .push(LegacyNormalization::MetadataIdentityCanonicalized {
                        asset_id: asset.id.clone(),
                        field: field.clone(),
                    }),
                (Some(_), None) => {}
                _ => bad("must equal the immutable resource byte size"),
            },
            "media_url" => {
                let hash = version
                    .map(|v| v.content_hash.as_str())
                    .or_else(|| asset.metadata.get("content_hash").and_then(|v| v.as_str()));
                if hash.is_some_and(|hash| {
                    value.as_str() == Some(format!("/v1/media/{hash}").as_str())
                }) {
                    context
                        .report
                        .normalizations
                        .push(LegacyNormalization::MediaUrlDerived {
                            asset_id: asset.id.clone(),
                        });
                } else {
                    bad("only the standard CAS-derived /v1/media/{content_hash} URL is understood");
                }
            }
            "mime_type" => match value.as_str() {
                Some(value) if !value.trim().is_empty() => result.mime_type = Some(value.into()),
                _ => bad("must be a nonempty media type string"),
            },
            "has_audio" => match value.as_bool() {
                Some(true) => result.audio = AudioPresenceV4::Present { metadata: None },
                Some(false) => result.audio = AudioPresenceV4::Absent {},
                None => bad("must be a boolean probe observation"),
            },
            "frame_rate" => match serde_json::from_value::<FrameRate>(value.clone()) {
                Ok(rate)
                    if rate.numerator > 0
                        && rate.denominator > 0
                        && asset.kind == AssetKind::Video =>
                {
                    result.frame_rate = Some(rate)
                }
                _ => bad("requires a positive rational video frame rate"),
            },
            "width" | "height" => {
                if value.is_null() {
                    continue;
                }
                match value
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0)
                {
                    Some(number) => {
                        if field == "width" {
                            width = Some(number);
                        } else {
                            height = Some(number);
                        }
                    }
                    None => bad("must be null or a positive u32 pixel dimension"),
                }
            }
            _ => context.issue(
                location,
                LegacyConversionIssueKind::UnknownMetadata {
                    field: field.clone(),
                },
            ),
        }
    }
    // Map keys sort audio_metadata before has_audio; presence must be checked
    // after collecting the complete observation rather than inferred from it.
    if let Some(metadata) = audio_metadata {
        if matches!(result.audio, AudioPresenceV4::Present { .. }) {
            result.audio = AudioPresenceV4::Present {
                metadata: Some(metadata),
            };
        } else {
            context.issue(
                format!("{path}.metadata.audio_metadata"),
                LegacyConversionIssueKind::InvalidMetadata {
                    field: "audio_metadata".into(),
                    reason: "audio format requires an explicit has_audio=true probe observation"
                        .into(),
                },
            );
        }
    }
    match (width, height) {
        (Some(width), Some(height)) => result.intrinsic_size = Some(CanvasSizeV4 { width, height }),
        (None, None) => {}
        _ => context.issue(
            format!("{path}.metadata"),
            LegacyConversionIssueKind::InvalidMetadata {
                field: "width/height".into(),
                reason: "pixel dimensions must be jointly known or jointly unknown".into(),
            },
        ),
    }
    result
}
