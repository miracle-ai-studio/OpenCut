//! V4 whole-scene inference shares the Classic audio use case, never legacy projection.
use crate::{JobRecord, JobRequest, scene_audio_jobs as audio};
use opencut_contracts::{EditorError, Time};
use opencut_job_contracts::scene_transcription::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedSceneTranscription {
    pub audio: audio::PreparedAudioWav,
    pub language: Option<String>,
}

/// New provenance is private. The legacy unwrapped result remains readable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredSceneTranscription {
    pub public: SceneTranscriptionResult,
    pub audio_decoder_evidence_v1: crate::classic_audio::ClassicAudioDecoderEvidence,
}
pub fn validate_stored_result(
    input: &PreparedSceneTranscription,
    value: serde_json::Value,
) -> Result<SceneTranscriptionResult, EditorError> {
    if value.get("public").is_some() {
        let stored: StoredSceneTranscription = serde_json::from_value(value)
            .map_err(|_| invalid("Invalid private transcript evidence"))?;
        validate_result_with_evidence(
            input,
            &stored.public,
            Some(&stored.audio_decoder_evidence_v1),
        )?;
        Ok(stored.public)
    } else {
        let public: SceneTranscriptionResult =
            serde_json::from_value(value).map_err(|_| invalid("Invalid stored transcript"))?;
        validate_result(input, &public)?;
        Ok(public)
    }
}

pub trait SceneTranscriptionStore: audio::AudioJobSubmissionStore {
    fn accept_scene_transcription(
        &mut self,
        request: JobRequest,
        prepared: PreparedSceneTranscription,
    ) -> Result<JobRecord, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn validate_submission(input: &SceneTranscriptionRequest) -> Result<(), EditorError> {
    audio::validate_submission(&audio_input(input))?;
    validate_language(input.language.as_deref())
}
pub fn validate_language(language: Option<&str>) -> Result<(), EditorError> {
    if language.is_some_and(|s| {
        s != "auto" && (!(2..=3).contains(&s.len()) || !s.bytes().all(|b| b.is_ascii_lowercase()))
    }) {
        return Err(invalid(
            "Language must be auto or a lowercase two or three letter code",
        ));
    }
    Ok(())
}
fn audio_input(input: &SceneTranscriptionRequest) -> audio::SubmitAudioWav {
    audio::SubmitAudioWav {
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        sequence_id: input.sequence_id.clone(),
        idempotency_key: input.idempotency_key.clone(),
        sample_rate: 48_000,
    }
}
struct Capture<'a, S> {
    store: &'a mut S,
    request: JobRequest,
    language: Option<String>,
}
impl<S: SceneTranscriptionStore> audio::AudioJobSubmissionStore for Capture<'_, S> {
    fn fixed_scene_audio_snapshot(
        &self,
        p: &str,
        r: &str,
    ) -> Result<audio::AuthorizedAudioSnapshot, EditorError> {
        self.store.fixed_scene_audio_snapshot(p, r)
    }
    fn find_accepted_scene_audio_job(
        &self,
        _: &JobRequest,
    ) -> Result<Option<JobRecord>, EditorError> {
        self.store.find_accepted_scene_audio_job(&self.request)
    }
    fn accept_scene_audio_job(
        &mut self,
        _: JobRequest,
        prepared: audio::PreparedAudioWav,
    ) -> Result<JobRecord, EditorError> {
        self.store.accept_scene_transcription(
            self.request.clone(),
            PreparedSceneTranscription {
                audio: prepared,
                language: self.language.clone(),
            },
        )
    }
}
pub fn submit(
    store: &mut impl SceneTranscriptionStore,
    assets: &impl audio::AudioAssetCapture,
    input: SceneTranscriptionRequest,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let request = JobRequest {
        kind: SCENE_TRANSCRIPTION_KIND.into(),
        idempotency_key: input.idempotency_key.clone(),
        input: serde_json::to_value(&input)
            .map_err(|_| invalid("Cannot serialize transcription request"))?,
    };
    let mut capture = Capture {
        store,
        request,
        language: input.language.clone(),
    };
    audio::submit_audio_wav(&mut capture, assets, audio_input(&input))
}
pub fn validate_prepared(input: &PreparedSceneTranscription) -> Result<(), EditorError> {
    validate_language(input.language.as_deref())?;
    if input.audio.sample_rate != 48_000 || input.audio.output_range.is_some() {
        return Err(invalid(
            "Transcription requires the complete 48000 Hz Classic mix",
        ));
    }
    audio::validate_prepared(&input.audio)?;
    Ok(())
}
pub fn validate_request(
    request: &JobRequest,
    prepared: &PreparedSceneTranscription,
) -> Result<(), EditorError> {
    let input: SceneTranscriptionRequest = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid V4 transcription request"))?;
    validate_submission(&input)?;
    if request.kind != SCENE_TRANSCRIPTION_KIND
        || request.idempotency_key != input.idempotency_key
        || input.language != prepared.language
    {
        return Err(invalid(
            "Transcription preparation differs from original request",
        ));
    }
    let audio_request = JobRequest {
        kind: audio::AUDIO_WAV_KIND.into(),
        idempotency_key: input.idempotency_key.clone(),
        input: serde_json::to_value(audio_input(&input))
            .map_err(|_| invalid("Cannot serialize audio request"))?,
    };
    audio::validate_prepared_request(&audio_request, &prepared.audio)?;
    validate_prepared(prepared)
}
pub fn validate_result(
    input: &PreparedSceneTranscription,
    result: &SceneTranscriptionResult,
) -> Result<(), EditorError> {
    validate_result_with_evidence(input, result, None)
}
fn validate_result_with_evidence(
    input: &PreparedSceneTranscription,
    result: &SceneTranscriptionResult,
    evidence: Option<&crate::classic_audio::ClassicAudioDecoderEvidence>,
) -> Result<(), EditorError> {
    validate_prepared(input)?;
    let frames = audio::validate_prepared(&input.audio)?.duration_samples();
    if result.schema_version != 1
        || result.project_id != input.audio.snapshot.project.id
        || result.revision != input.audio.snapshot.revision
        || result.sequence_id != input.audio.sequence_id
        || result.duration
            != (Time {
                ticks: frames.to_string(),
                timebase: 48_000,
            })
        || !opencut_media_contracts::validate_public_transcription_result(&result.transcription)
    {
        return Err(EditorError::new(
            "INVALID_STORED_RESULT",
            "Transcript differs from its fixed V4 audio input",
        ));
    }
    let stored = audio::StoredAudioWavResult {
        audio_decoder_evidence_v1: evidence.cloned(),
        public: result.audio.clone(),
        artifact: opencut_job_contracts::ArtifactDescriptor {
            path: "private-audio.wav".into(),
            mime_type: result.audio.artifact.mime_type.clone(),
            size: result
                .audio
                .artifact
                .size_bytes
                .parse()
                .map_err(|_| invalid("Invalid audio evidence size"))?,
            content_hash: result.audio.artifact.content_hash.clone(),
        },
    };
    audio::validate_stored_result(&input.audio, &stored)?;
    Ok(())
}
