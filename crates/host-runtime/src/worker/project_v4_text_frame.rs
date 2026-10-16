//! Fixed complete Classic media/free-text frame execution. Private PNG publication uses the queue fence.
use super::output_attempt::{
    attempt_directory, cancelled, check_attempt, check_output_name, error, file_hash, io_error,
};
use crate::classic_visual_runtime;
use opencut_composition::project_v4::{
    PreparedBlendFrameV1, PreparedGraphicFrameV1, PreparedMaskFrameV1, PreparedSceneFrameV1,
    PreparedTextFrameV2, PreparedTextFrameV3,
};
use opencut_contracts::EditorError;
use opencut_editor_application::{
    budget::Writer,
    classic_blend_frame_v1::{ClassicBlendFrameExecutorV1, ClassicBlendFrameSessionV1},
    classic_graphic_frame_v1::{ClassicGraphicFrameExecutorV1, ClassicGraphicFrameSessionV1},
    classic_mask_frame_v1::{ClassicMaskFrameExecutorV1, ClassicMaskFrameSessionV1},
    classic_scene_frame_v1::{ClassicSceneFrameExecutorV1, ClassicSceneFrameSessionV1},
    classic_text_frame_v2::{
        ClassicTextFrameExecutorV2, ClassicTextFrameSessionV2, admit_text_frame_v2,
    },
    classic_text_frame_v3::{ClassicCaptionFrameExecutorV3, ClassicCaptionFrameSessionV3},
    classic_visual::ClassicVisualResources,
    font_import_v1::FontImportPolicyV1,
    project_v4_blend_frame_jobs::{self as blend, StoredV4BlendFramePngResultV1},
    project_v4_caption_frame_jobs::{self as caption, StoredV4CaptionFramePngResultV3},
    project_v4_graphic_frame_jobs::{self as graphic, StoredV4GraphicFramePngResultV1},
    project_v4_mask_frame_jobs::{self as mask, StoredV4MaskFramePngResultV1},
    project_v4_scene_frame_jobs::{self as scene, StoredV4SceneFramePngResultV1},
    project_v4_text_frame_jobs::{
        self as frame, PreparedV4TextFramePng, StoredV4TextFramePngResultV2,
    },
};
use opencut_job_contracts::{
    ArtifactDescriptor,
    project_v4_blend_frame::{V4_BLEND_FRAME_PNG_KIND, V4BlendFramePngResultV1},
    project_v4_caption_frame::{V4_CAPTION_FRAME_PNG_KIND, V4CaptionFramePngResultV3},
    project_v4_frame::{V4FrameArtifactMetadataV1, V4FrameExecutionSummaryV1},
    project_v4_graphic_frame::{V4_GRAPHIC_FRAME_PNG_KIND, V4GraphicFramePngResultV1},
    project_v4_mask_frame::{V4_MASK_FRAME_PNG_KIND, V4MaskFramePngResultV1},
    project_v4_scene_frame::{V4_SCENE_FRAME_PNG_KIND, V4SceneFramePngResultV1},
    project_v4_text_frame::*,
};
use opencut_media::CancellationToken;
use opencut_native_services::classic_visual::{
    ClassicVisualPngSink, ClassicVisualProcess, VerifiedClassicTextResources,
};
use opencut_sqlite_store::{EditorApplication, JobRecord};
use opencut_storage_budget::{AttemptLease, Budget};
use serde_json::Value;
use std::{ffi::OsStr, path::Path, sync::Mutex};

enum PreparedFrame {
    Text(PreparedTextFrameV2),
    Caption(PreparedTextFrameV3),
    Graphic(PreparedGraphicFrameV1),
    Mask(PreparedMaskFrameV1),
    Scene(PreparedSceneFrameV1),
    Blend(PreparedBlendFrameV1),
}
impl PreparedFrame {
    fn execution(&self) -> &PreparedTextFrameV2 {
        match self {
            Self::Text(frame) => frame,
            Self::Caption(frame) => frame.execution(),
            Self::Graphic(frame) => frame.execution().execution(),
            Self::Mask(frame) => frame.execution().execution().execution(),
            Self::Scene(frame) => frame.execution().execution().execution(),
            Self::Blend(frame) => frame.execution().execution().execution(),
        }
    }
}

pub(super) fn execute(
    database: &Path,
    root: &Path,
    worker: &str,
    job: &JobRecord,
    cancel: &CancellationToken,
    held: &Mutex<Option<AttemptLease>>,
) -> Result<Value, EditorError> {
    cancelled(cancel)?;
    let input: PreparedV4TextFramePng = serde_json::from_value(job.input.clone())
        .map_err(|_| error("INVALID_ARGUMENT", "Invalid private V4 frame preparation"))?;
    let is_caption = job.kind == V4_CAPTION_FRAME_PNG_KIND;
    let is_graphic = job.kind == V4_GRAPHIC_FRAME_PNG_KIND;
    let is_mask = job.kind == V4_MASK_FRAME_PNG_KIND;
    let is_scene = job.kind == V4_SCENE_FRAME_PNG_KIND;
    let is_blend = job.kind == V4_BLEND_FRAME_PNG_KIND;
    let prepared = if is_scene {
        PreparedFrame::Scene(scene::validate_prepared(&input)?)
    } else if is_mask {
        PreparedFrame::Mask(mask::validate_prepared(&input)?)
    } else if is_blend {
        PreparedFrame::Blend(blend::validate_prepared(&input)?)
    } else if is_graphic {
        PreparedFrame::Graphic(graphic::validate_prepared(&input)?)
    } else if is_caption {
        PreparedFrame::Caption(caption::validate_prepared(&input)?)
    } else if job.kind == V4_TEXT_FRAME_PNG_KIND {
        PreparedFrame::Text(frame::validate_prepared(&input)?)
    } else {
        return Err(error(
            "INVALID_ARGUMENT",
            "Unexpected text/caption/graphic frame job kind",
        ));
    };
    let app = EditorApplication::open_managed(database)?;
    let revalidate = || {
        if is_scene {
            app.revalidate_v4_scene_frame_job(&job.job_id, worker, job.attempt, &input.authority)
        } else if is_mask {
            app.revalidate_v4_mask_frame_job(&job.job_id, worker, job.attempt, &input.authority)
        } else if is_blend {
            app.revalidate_v4_blend_frame_job(&job.job_id, worker, job.attempt, &input.authority)
        } else if is_graphic {
            app.revalidate_v4_graphic_frame_job(&job.job_id, worker, job.attempt, &input.authority)
        } else if is_caption {
            app.revalidate_v4_caption_frame_job(&job.job_id, worker, job.attempt, &input.authority)
        } else {
            app.revalidate_v4_text_frame_job(&job.job_id, worker, job.attempt, &input.authority)
        }
    };
    revalidate()?;
    // Complete-frame admission precedes opening any media or font descriptor.
    admit_text_frame_v2(prepared.execution(), &input.processing_policy)?;
    let config = if is_scene {
        classic_visual_runtime::configured_scene(&input.processing_policy)?
    } else if is_mask {
        classic_visual_runtime::configured_mask(&input.processing_policy)?
    } else if is_blend {
        classic_visual_runtime::configured_blend(&input.processing_policy)?
    } else if is_graphic {
        classic_visual_runtime::configured_graphic(&input.processing_policy)?
    } else if is_caption {
        classic_visual_runtime::configured_caption(&input.processing_policy)?
    } else {
        classic_visual_runtime::configured_text(&input.processing_policy)?
    };
    let identity = prepared.execution().identity();
    let ceiling = ClassicVisualPngSink::output_ceiling(identity.width, identity.height)?;
    if ceiling > input.processing_policy.max_output_bytes {
        return Err(error(
            "VISUAL_BUDGET_EXCEEDED",
            "PNG ceiling exceeds captured output admission",
        ));
    }
    let workspace = root
        .parent()
        .ok_or_else(|| error("STORAGE_BUDGET_ERROR", "Missing managed output workspace"))?;
    let _reservation = Budget::new(workspace).reserve_output_bytes(
        ceiling,
        Writer {
            job_id: job.job_id.clone(),
            attempt: job.attempt,
        },
    )?;
    let directory = attempt_directory(root, job)?;
    let attempt = AttemptLease::acquire(&directory).map_err(io_error)?;
    check_attempt(&attempt, &directory)?;
    {
        let mut guard = held
            .lock()
            .map_err(|_| error("INTERNAL_ERROR", "Attempt lifetime guard unavailable"))?;
        if guard.is_some() {
            return Err(error("INTERNAL_ERROR", "Output attempt already held"));
        }
        *guard = Some(attempt.clone());
    }
    let checkpoint = || {
        cancelled(cancel)?;
        check_attempt(&attempt, &directory)?;
        revalidate()
    };
    let mut resources = match &prepared {
        PreparedFrame::Graphic(prepared) => VerifiedClassicTextResources::new_caption_frame_v3(
            app.asset_store_root(),
            prepared.execution(),
            &FontImportPolicyV1::default(),
            checkpoint,
        )?,
        PreparedFrame::Scene(prepared) => VerifiedClassicTextResources::new_scene_frame_v1(
            app.asset_store_root(),
            prepared,
            &FontImportPolicyV1::default(),
            checkpoint,
        )?,
        PreparedFrame::Mask(prepared) => VerifiedClassicTextResources::new_caption_frame_v3(
            app.asset_store_root(),
            prepared.execution().execution(),
            &FontImportPolicyV1::default(),
            checkpoint,
        )?,
        PreparedFrame::Blend(prepared) => VerifiedClassicTextResources::new_caption_frame_v3(
            app.asset_store_root(),
            prepared.execution().execution(),
            &FontImportPolicyV1::default(),
            checkpoint,
        )?,
        PreparedFrame::Text(prepared) => VerifiedClassicTextResources::new(
            app.asset_store_root(),
            prepared,
            &FontImportPolicyV1::default(),
            checkpoint,
        )?,
        PreparedFrame::Caption(prepared) => VerifiedClassicTextResources::new_caption_frame_v3(
            app.asset_store_root(),
            prepared,
            &FontImportPolicyV1::default(),
            checkpoint,
        )?,
    };
    let handles = resources.handles();
    let fonts = resources.font_handles();
    resources.checkpoint()?;
    let filename = OsStr::new("frame.png");
    let file = attempt.create_readwrite_child(filename).map_err(io_error)?;
    let mut sink = ClassicVisualPngSink::new(file, identity.width, identity.height, ceiling)?;
    let host_sha256 =
        opencut_native_services::reproducibility::execution_identity(vec![])?.process_binary_sha256;
    let process = ClassicVisualProcess { config };
    let (runtime_identity, bound, captions, graphics) = match &prepared {
        PreparedFrame::Graphic(prepared) => {
            let mut session = ClassicGraphicFrameSessionV1::new(
                prepared,
                &input.processing_policy,
                resources.coverage(),
            )?;
            let execution = process.execute_graphic_frame(
                &mut session,
                &mut resources,
                &handles,
                &fonts,
                &mut sink,
            )?;
            (
                execution.runtime_identity,
                execution.receipt.execution.execution.bound,
                Some(execution.receipt.execution.captions),
                Some(execution.receipt.graphics),
            )
        }
        PreparedFrame::Scene(prepared) => {
            let mut session = ClassicSceneFrameSessionV1::new(
                prepared,
                &input.processing_policy,
                resources.coverage(),
            )?;
            let execution = process.execute_scene_frame(
                &mut session,
                &mut resources,
                &handles,
                &fonts,
                &mut sink,
            )?;
            (
                execution.runtime_identity,
                execution
                    .receipt
                    .execution
                    .execution
                    .execution
                    .execution
                    .bound,
                Some(execution.receipt.execution.execution.execution.captions),
                Some(execution.receipt.execution.execution.graphics),
            )
        }
        PreparedFrame::Mask(prepared) => {
            let mut session = ClassicMaskFrameSessionV1::new(
                prepared,
                &input.processing_policy,
                resources.coverage(),
            )?;
            let execution = process.execute_mask_frame(
                &mut session,
                &mut resources,
                &handles,
                &fonts,
                &mut sink,
            )?;
            (
                execution.runtime_identity,
                execution
                    .receipt
                    .execution
                    .execution
                    .execution
                    .execution
                    .bound,
                Some(execution.receipt.execution.execution.execution.captions),
                Some(execution.receipt.execution.execution.graphics),
            )
        }
        PreparedFrame::Blend(prepared) => {
            let mut session = ClassicBlendFrameSessionV1::new(
                prepared,
                &input.processing_policy,
                resources.coverage(),
            )?;
            let execution = process.execute_blend_frame(
                &mut session,
                &mut resources,
                &handles,
                &fonts,
                &mut sink,
            )?;
            (
                execution.runtime_identity,
                execution.receipt.execution.execution.execution.bound,
                Some(execution.receipt.execution.execution.captions),
                Some(execution.receipt.execution.graphics),
            )
        }
        PreparedFrame::Text(prepared) => {
            let mut session = ClassicTextFrameSessionV2::new(
                prepared,
                &input.processing_policy,
                resources.coverage(),
            )?;
            let execution = process.execute_text_frame(
                &mut session,
                &mut resources,
                &handles,
                &fonts,
                &mut sink,
            )?;
            (
                execution.runtime_identity,
                execution.receipt.bound,
                None,
                None,
            )
        }
        PreparedFrame::Caption(prepared) => {
            let mut session = ClassicCaptionFrameSessionV3::new(
                prepared,
                &input.processing_policy,
                resources.coverage(),
            )?;
            let execution = process.execute_caption_frame(
                &mut session,
                &mut resources,
                &handles,
                &fonts,
                &mut sink,
            )?;
            (
                execution.runtime_identity,
                execution.receipt.execution.bound,
                Some(execution.receipt.captions),
                None,
            )
        }
    };
    resources.checkpoint()?;
    let mut file = sink.finalize()?;
    let size = file.metadata().map_err(io_error)?.len();
    if size == 0 || size > ceiling {
        return Err(error(
            "ARTIFACT_VERIFICATION_FAILED",
            "PNG exceeds captured byte admission",
        ));
    }
    let content_hash = file_hash(&mut file, size, cancel)?;
    attempt.sync_directory().map_err(io_error)?;
    check_output_name(&attempt, filename, &file)?;
    resources.checkpoint()?;
    let runtime = runtime_identity.identity;
    let summary = V4FrameExecutionSummaryV1 {
        host_sha256: host_sha256.clone(),
        browser_version: runtime.browser_version,
        browser_sha256: runtime.browser_sha256,
        module_sha256: runtime.module_sha256,
        shader_sha256: runtime.shader_sha256,
        playwright_version: runtime.playwright_version,
        mediabunny_version: runtime.mediabunny_version,
        gpu_backend: runtime.gpu_backend,
        gpu_device: runtime.gpu_device,
        texture_format: runtime.texture_format,
        decoder_profile: runtime.decoder_profile,
    };
    let sources = opencut_editor_application::project_v4_frame_jobs::public_sources(
        prepared.execution().media(),
        &bound.decoded,
    )?;
    let metadata = V4FrameArtifactMetadataV1 {
        mime_type: "image/png".into(),
        size_bytes: size.to_string(),
        content_hash: content_hash.clone(),
    };
    let artifact = ArtifactDescriptor {
        path: directory
            .join(filename)
            .into_os_string()
            .into_string()
            .map_err(|_| {
                error(
                    "ARTIFACT_VERIFICATION_FAILED",
                    "Private output path is not UTF-8",
                )
            })?,
        mime_type: "image/png".into(),
        size,
        content_hash,
    };
    let result = match &prepared {
        PreparedFrame::Graphic(prepared) => {
            let stored = StoredV4GraphicFramePngResultV1 {
                public: V4GraphicFramePngResultV1 {
                    schema_version: 1,
                    identity: graphic::public_identity(prepared),
                    asset_lock: input.asset_lock.clone(),
                    font_lock: input.font_lock.clone(),
                    render_input_sha256: graphic::render_input_sha256(
                        &input.snapshot,
                        &input.sequence_id,
                        &input.time,
                    )?,
                    execution: summary,
                    sources,
                    texts: bound.rasters,
                    captions: captions.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing caption partition")
                    })?,
                    graphics: graphics.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing verified graphic rasters")
                    })?,
                    artifact: metadata,
                },
                artifact,
            };
            graphic::validate_stored_result(&input, &stored)?;
            serde_json::to_value(stored)
        }
        PreparedFrame::Scene(prepared) => {
            let stored = StoredV4SceneFramePngResultV1 {
                public: V4SceneFramePngResultV1 {
                    schema_version: 1,
                    identity: scene::public_identity(prepared),
                    asset_lock: input.asset_lock.clone(),
                    font_lock: input.font_lock.clone(),
                    render_input_sha256: scene::render_input_sha256(
                        &input.snapshot,
                        &input.sequence_id,
                        &input.time,
                    )?,
                    execution: summary,
                    sources,
                    texts: bound.rasters,
                    captions: captions.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing caption partition")
                    })?,
                    blends: prepared.blends().to_vec(),
                    stacks: prepared.stacks().to_vec(),
                    steps: prepared.steps().to_vec(),
                    background: prepared.background().clone(),
                    graphics: graphics.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing verified scene rasters")
                    })?,
                    artifact: metadata,
                },
                artifact,
            };
            scene::validate_stored_result(&input, &stored)?;
            serde_json::to_value(stored)
        }
        PreparedFrame::Mask(prepared) => {
            let stored = StoredV4MaskFramePngResultV1 {
                public: V4MaskFramePngResultV1 {
                    schema_version: 1,
                    identity: mask::public_identity(prepared),
                    asset_lock: input.asset_lock.clone(),
                    font_lock: input.font_lock.clone(),
                    render_input_sha256: mask::render_input_sha256(
                        &input.snapshot,
                        &input.sequence_id,
                        &input.time,
                    )?,
                    execution: summary,
                    sources,
                    texts: bound.rasters,
                    captions: captions.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing caption partition")
                    })?,
                    blends: prepared.blends().to_vec(),
                    masks: prepared.masks().to_vec(),
                    graphics: graphics.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing verified mask rasters")
                    })?,
                    artifact: metadata,
                },
                artifact,
            };
            mask::validate_stored_result(&input, &stored)?;
            serde_json::to_value(stored)
        }
        PreparedFrame::Blend(prepared) => {
            let stored = StoredV4BlendFramePngResultV1 {
                public: V4BlendFramePngResultV1 {
                    schema_version: 1,
                    identity: blend::public_identity(prepared),
                    asset_lock: input.asset_lock.clone(),
                    font_lock: input.font_lock.clone(),
                    render_input_sha256: blend::render_input_sha256(
                        &input.snapshot,
                        &input.sequence_id,
                        &input.time,
                    )?,
                    execution: summary,
                    sources,
                    texts: bound.rasters,
                    captions: captions.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing caption partition")
                    })?,
                    blends: prepared.blends().to_vec(),
                    graphics: graphics.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing verified blend rasters")
                    })?,
                    artifact: metadata,
                },
                artifact,
            };
            blend::validate_stored_result(&input, &stored)?;
            serde_json::to_value(stored)
        }
        PreparedFrame::Text(prepared) => {
            let stored = StoredV4TextFramePngResultV2 {
                public: V4TextFramePngResultV2 {
                    schema_version: 2,
                    identity: frame::public_identity(prepared),
                    asset_lock: input.asset_lock.clone(),
                    font_lock: input.font_lock.clone(),
                    render_input_sha256: frame::render_input_sha256(
                        &input.snapshot,
                        &input.sequence_id,
                        &input.time,
                    )?,
                    execution: summary,
                    sources,
                    texts: bound.rasters,
                    artifact: metadata,
                },
                artifact,
            };
            frame::validate_stored_result(&input, &stored)?;
            serde_json::to_value(stored)
        }
        PreparedFrame::Caption(prepared) => {
            let stored = StoredV4CaptionFramePngResultV3 {
                public: V4CaptionFramePngResultV3 {
                    schema_version: 3,
                    identity: caption::public_identity(prepared),
                    asset_lock: input.asset_lock.clone(),
                    font_lock: input.font_lock.clone(),
                    render_input_sha256: caption::render_input_sha256(
                        &input.snapshot,
                        &input.sequence_id,
                        &input.time,
                    )?,
                    execution: summary,
                    sources,
                    texts: bound.rasters,
                    captions: captions.ok_or_else(|| {
                        error("INVALID_STORED_RESULT", "Missing verified caption layouts")
                    })?,
                    artifact: metadata,
                },
                artifact,
            };
            caption::validate_stored_result(&input, &stored)?;
            serde_json::to_value(stored)
        }
    }
    .map_err(|_| {
        error(
            "SERIALIZATION_ERROR",
            "Could not serialize private frame result",
        )
    })?;
    resources.checkpoint()?;
    if opencut_native_services::reproducibility::execution_identity(vec![])?.process_binary_sha256
        != host_sha256
    {
        return Err(error(
            "EXECUTION_IDENTITY_CHANGED",
            "Executing host changed during frame processing",
        ));
    }
    check_output_name(&attempt, filename, &file)?;
    Ok(result)
}
