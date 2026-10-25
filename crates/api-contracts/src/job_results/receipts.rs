//! Public result bodies are distinct from native stored receipts. Native
//! validators retain their original tags and hashes;public bodies carry the
//! same observed facts without a development-generation envelope counter.

mod media_frame_result {
    use opencut_contracts::{AssetLockManifest, Time};
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameIdentity,
        MediaFrameSourceObservation,
    };
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MediaFrameResult {
        pub identity: MediaFrameIdentity,
        pub asset_lock: AssetLockManifest,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::media_frame::MediaFramePngResult> for MediaFrameResult {
        fn from(value: opencut_job_contracts::media_frame::MediaFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                artifact: value.artifact,
            }
        }
    }
}
pub use media_frame_result::MediaFrameResult;

mod text_frame_result {
    use opencut_contracts::text_frame::FontLock;
    use opencut_contracts::{AssetLockManifest, Time, text_frame::TextRasterReceipt};
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
    };
    use opencut_job_contracts::scene_text_frame::TextFrameJobIdentity;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct TextFrameResult {
        pub identity: TextFrameJobIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub texts: Vec<TextRasterReceipt>,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_text_frame::TextFramePngResult> for TextFrameResult {
        fn from(value: opencut_job_contracts::scene_text_frame::TextFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                texts: value.texts,
                artifact: value.artifact,
            }
        }
    }
}
pub use text_frame_result::TextFrameResult;

mod caption_frame_result {
    use opencut_contracts::{
        AssetLockManifest, Time,
        caption_frame::CaptionLayoutReceipt,
        text_frame::{FontLock, TextRasterReceipt},
    };
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
    };
    use opencut_job_contracts::scene_caption_frame::CaptionFrameIdentity;
    use opencut_job_contracts::scene_text_frame::FrameRenderInput;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct CaptionFrameResult {
        pub identity: CaptionFrameIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub texts: Vec<TextRasterReceipt>,
        pub captions: Vec<CaptionLayoutReceipt>,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_caption_frame::CaptionFramePngResult>
        for CaptionFrameResult
    {
        fn from(value: opencut_job_contracts::scene_caption_frame::CaptionFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                texts: value.texts,
                captions: value.captions,
                artifact: value.artifact,
            }
        }
    }
}
pub use caption_frame_result::CaptionFrameResult;

mod graphic_frame_result {
    use opencut_contracts::{
        AssetLockManifest, Time,
        caption_frame::CaptionLayoutReceipt,
        graphic_frame::GraphicRasterReceipt,
        text_frame::{FontLock, TextRasterReceipt},
    };
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
    };
    use opencut_job_contracts::scene_graphic_frame::GraphicFrameIdentity;
    use opencut_job_contracts::scene_text_frame::FrameRenderInput;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct GraphicFrameResult {
        pub identity: GraphicFrameIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub texts: Vec<TextRasterReceipt>,
        pub captions: Vec<CaptionLayoutReceipt>,
        pub graphics: Vec<GraphicRasterReceipt>,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_graphic_frame::GraphicFramePngResult>
        for GraphicFrameResult
    {
        fn from(value: opencut_job_contracts::scene_graphic_frame::GraphicFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                texts: value.texts,
                captions: value.captions,
                graphics: value.graphics,
                artifact: value.artifact,
            }
        }
    }
}
pub use graphic_frame_result::GraphicFrameResult;

mod mask_frame_result {
    use opencut_contracts::{
        AssetLockManifest, Time,
        blend_frame::LayerBlend,
        caption_frame::CaptionLayoutReceipt,
        graphic_frame::GraphicRasterReceipt,
        mask_frame::LayerMask,
        text_frame::{FontLock, TextRasterReceipt},
    };
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
    };
    use opencut_job_contracts::scene_mask_frame::MaskFrameIdentity;
    use opencut_job_contracts::scene_text_frame::FrameRenderInput;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MaskFrameResult {
        pub identity: MaskFrameIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub texts: Vec<TextRasterReceipt>,
        pub captions: Vec<CaptionLayoutReceipt>,
        pub graphics: Vec<GraphicRasterReceipt>,
        pub blends: Vec<LayerBlend>,
        pub masks: Vec<LayerMask>,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_mask_frame::MaskFramePngResult> for MaskFrameResult {
        fn from(value: opencut_job_contracts::scene_mask_frame::MaskFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                texts: value.texts,
                captions: value.captions,
                graphics: value.graphics,
                blends: value.blends,
                masks: value.masks,
                artifact: value.artifact,
            }
        }
    }
}
pub use mask_frame_result::MaskFrameResult;

mod blend_frame_result {
    use opencut_contracts::{
        AssetLockManifest, Time,
        blend_frame::LayerBlend,
        caption_frame::CaptionLayoutReceipt,
        graphic_frame::GraphicRasterReceipt,
        text_frame::{FontLock, TextRasterReceipt},
    };
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
    };
    use opencut_job_contracts::scene_blend_frame::BlendFrameIdentity;
    use opencut_job_contracts::scene_text_frame::FrameRenderInput;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct BlendFrameResult {
        pub identity: BlendFrameIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub texts: Vec<TextRasterReceipt>,
        pub captions: Vec<CaptionLayoutReceipt>,
        pub graphics: Vec<GraphicRasterReceipt>,
        pub blends: Vec<LayerBlend>,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_blend_frame::BlendFramePngResult> for BlendFrameResult {
        fn from(value: opencut_job_contracts::scene_blend_frame::BlendFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                texts: value.texts,
                captions: value.captions,
                graphics: value.graphics,
                blends: value.blends,
                artifact: value.artifact,
            }
        }
    }
}
pub use blend_frame_result::BlendFrameResult;

mod scene_frame_result {
    use opencut_contracts::{
        AssetLockManifest, Time,
        blend_frame::LayerBlend,
        caption_frame::CaptionLayoutReceipt,
        graphic_frame::GraphicRasterReceipt,
        scene_frame::{SceneBackground, SceneLayerStack, SceneStep},
        text_frame::{FontLock, TextRasterReceipt},
    };
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary, MediaFrameSourceObservation,
    };
    use opencut_job_contracts::scene_frame::SceneFrameIdentity;
    use opencut_job_contracts::scene_text_frame::FrameRenderInput;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SceneFrameResult {
        pub identity: SceneFrameIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaFrameExecutionSummary,
        pub sources: Vec<MediaFrameSourceObservation>,
        pub texts: Vec<TextRasterReceipt>,
        pub captions: Vec<CaptionLayoutReceipt>,
        pub graphics: Vec<GraphicRasterReceipt>,
        pub blends: Vec<LayerBlend>,
        pub stacks: Vec<SceneLayerStack>,
        pub steps: Vec<SceneStep>,
        pub background: SceneBackground,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_frame::SceneFramePngResult> for SceneFrameResult {
        fn from(value: opencut_job_contracts::scene_frame::SceneFramePngResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                sources: value.sources,
                texts: value.texts,
                captions: value.captions,
                graphics: value.graphics,
                blends: value.blends,
                stacks: value.stacks,
                steps: value.steps,
                background: value.background,
                artifact: value.artifact,
            }
        }
    }
}
pub use scene_frame_result::SceneFrameResult;

mod media_video_result {
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::media_frame::{
        MediaFrameArtifactMetadata, MediaFrameExecutionSummary,
    };
    use opencut_job_contracts::media_video::{MediaVideoExecutionSummary, MediaVideoIdentity};
    use opencut_job_contracts::scene_audio::AudioExecutionSummary;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MediaVideoResult {
        pub identity: MediaVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaVideoExecutionSummary,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::media_video::MediaVideoMp4Result> for MediaVideoResult {
        fn from(value: opencut_job_contracts::media_video::MediaVideoMp4Result) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use media_video_result::MediaVideoResult;

mod caption_video_result {
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::media_frame::MediaFrameArtifactMetadata;
    use opencut_job_contracts::media_video::{FixedVideoInput, MediaVideoExecutionSummary};
    use opencut_job_contracts::scene_caption_video::CaptionVideoIdentity;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct CaptionVideoResult {
        pub identity: CaptionVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaVideoExecutionSummary,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_caption_video::CaptionVideoMp4Result>
        for CaptionVideoResult
    {
        fn from(value: opencut_job_contracts::scene_caption_video::CaptionVideoMp4Result) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use caption_video_result::CaptionVideoResult;

mod graphic_video_result {
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::media_frame::MediaFrameArtifactMetadata;
    use opencut_job_contracts::media_video::{FixedVideoInput, MediaVideoExecutionSummary};
    use opencut_job_contracts::scene_graphic_video::GraphicVideoIdentity;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct GraphicVideoResult {
        pub identity: GraphicVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaVideoExecutionSummary,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_graphic_video::GraphicVideoMp4Result>
        for GraphicVideoResult
    {
        fn from(value: opencut_job_contracts::scene_graphic_video::GraphicVideoMp4Result) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use graphic_video_result::GraphicVideoResult;

mod mask_video_result {
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::media_frame::MediaFrameArtifactMetadata;
    use opencut_job_contracts::media_video::{FixedVideoInput, MediaVideoExecutionSummary};
    use opencut_job_contracts::scene_mask_video::MaskVideoIdentity;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct MaskVideoResult {
        pub identity: MaskVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaVideoExecutionSummary,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_mask_video::MaskVideoMp4Result> for MaskVideoResult {
        fn from(value: opencut_job_contracts::scene_mask_video::MaskVideoMp4Result) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use mask_video_result::MaskVideoResult;

mod blend_video_result {
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::media_frame::MediaFrameArtifactMetadata;
    use opencut_job_contracts::media_video::{FixedVideoInput, MediaVideoExecutionSummary};
    use opencut_job_contracts::scene_blend_video::BlendVideoIdentity;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct BlendVideoResult {
        pub identity: BlendVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaVideoExecutionSummary,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_blend_video::BlendVideoMp4Result> for BlendVideoResult {
        fn from(value: opencut_job_contracts::scene_blend_video::BlendVideoMp4Result) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use blend_video_result::BlendVideoResult;

mod scene_video_result {
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::media_frame::MediaFrameArtifactMetadata;
    use opencut_job_contracts::media_video::{FixedVideoInput, MediaVideoExecutionSummary};
    use opencut_job_contracts::scene_video::SceneVideoIdentity;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct SceneVideoResult {
        pub identity: SceneVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: MediaVideoExecutionSummary,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::scene_video::SceneVideoMp4Result> for SceneVideoResult {
        fn from(value: opencut_job_contracts::scene_video::SceneVideoMp4Result) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use scene_video_result::SceneVideoResult;

mod video_export_result {
    use opencut_contracts::export_video::*;
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::export_video::{ExportVideoExecution, ExportVideoIdentity};
    use opencut_job_contracts::{
        media_frame::{MediaFrameArtifactMetadata, MediaFrameExecutionSummary},
        scene_audio::AudioExecutionSummary,
    };
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct VideoExportResult {
        pub identity: ExportVideoIdentity,
        pub asset_lock: AssetLockManifest,
        pub font_lock: Vec<FontLock>,
        pub render_input_sha256: String,
        pub execution: ExportVideoExecution,
        pub artifact: MediaFrameArtifactMetadata,
    }
    impl From<opencut_job_contracts::export_video::ExportVideoResult> for VideoExportResult {
        fn from(value: opencut_job_contracts::export_video::ExportVideoResult) -> Self {
            Self {
                identity: value.identity,
                asset_lock: value.asset_lock,
                font_lock: value.font_lock,
                render_input_sha256: value.render_input_sha256,
                execution: value.execution,
                artifact: value.artifact,
            }
        }
    }
}
pub use video_export_result::VideoExportResult;

mod video_range_result {
    use opencut_contracts::export_video::*;
    use opencut_contracts::{AssetLockManifest, FrameRate, Time, text_frame::FontLock};
    use opencut_job_contracts::export_video::ExportRangeMapping;
    use opencut_job_contracts::{
        media_frame::{MediaFrameArtifactMetadata, MediaFrameExecutionSummary},
        scene_audio::AudioExecutionSummary,
    };
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct VideoRangeResult {
        pub mapping: ExportRangeMapping,
        /// Output frame/sample counts and duration; timeline_duration is the original scene extent.
        pub output: super::VideoExportResult,
    }
    impl From<opencut_job_contracts::export_video::ExportVideoRangeResult> for VideoRangeResult {
        fn from(value: opencut_job_contracts::export_video::ExportVideoRangeResult) -> Self {
            Self {
                mapping: value.mapping,
                output: value.output.into(),
            }
        }
    }
}
pub use video_range_result::VideoRangeResult;
