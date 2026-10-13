//! V4 whole-scene inference shares the Classic audio use case, never legacy projection.
use crate::{JobRecord, JobRequest, project_v4_audio_jobs as audio};
use opencut_contracts::{EditorError, Time};
use opencut_job_contracts::sequence_transcribe_v4::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedSequenceTranscribeV4 {
    pub audio: audio::PreparedV4AudioWav,
    pub language: Option<String>,
}

pub trait SequenceTranscribeStoreV4: audio::V4AudioJobSubmissionStore {
    fn accept_sequence_transcribe_v4(
        &mut self,
        request: JobRequest,
        prepared: PreparedSequenceTranscribeV4,
    ) -> Result<JobRecord, EditorError>;
}
fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
pub fn validate_submission(input: &SubmitSequenceTranscribeV4) -> Result<(), EditorError> {
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
fn audio_input(input: &SubmitSequenceTranscribeV4) -> audio::SubmitV4AudioWav {
    audio::SubmitV4AudioWav {
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
impl<S: SequenceTranscribeStoreV4> audio::V4AudioJobSubmissionStore for Capture<'_, S> {
    fn fixed_v4_audio_snapshot(
        &self,
        p: &str,
        r: &str,
    ) -> Result<audio::AuthorizedV4AudioSnapshot, EditorError> {
        self.store.fixed_v4_audio_snapshot(p, r)
    }
    fn find_accepted_v4_audio_job(&self, _: &JobRequest) -> Result<Option<JobRecord>, EditorError> {
        self.store.find_accepted_v4_audio_job(&self.request)
    }
    fn accept_v4_audio_job(
        &mut self,
        _: JobRequest,
        prepared: audio::PreparedV4AudioWav,
    ) -> Result<JobRecord, EditorError> {
        self.store.accept_sequence_transcribe_v4(
            self.request.clone(),
            PreparedSequenceTranscribeV4 {
                audio: prepared,
                language: self.language.clone(),
            },
        )
    }
}
pub fn submit(
    store: &mut impl SequenceTranscribeStoreV4,
    assets: &impl audio::V4AudioAssetCapture,
    input: SubmitSequenceTranscribeV4,
) -> Result<JobRecord, EditorError> {
    validate_submission(&input)?;
    let request = JobRequest {
        kind: SEQUENCE_TRANSCRIBE_V4_KIND.into(),
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
pub fn validate_prepared(input: &PreparedSequenceTranscribeV4) -> Result<(), EditorError> {
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
    prepared: &PreparedSequenceTranscribeV4,
) -> Result<(), EditorError> {
    let input: SubmitSequenceTranscribeV4 = serde_json::from_value(request.input.clone())
        .map_err(|_| invalid("Invalid V4 transcription request"))?;
    validate_submission(&input)?;
    if request.kind != SEQUENCE_TRANSCRIBE_V4_KIND
        || request.idempotency_key != input.idempotency_key
        || input.language != prepared.language
    {
        return Err(invalid(
            "Transcription preparation differs from original request",
        ));
    }
    let audio_request = JobRequest {
        kind: audio::V4_AUDIO_WAV_KIND.into(),
        idempotency_key: input.idempotency_key.clone(),
        input: serde_json::to_value(audio_input(&input))
            .map_err(|_| invalid("Cannot serialize audio request"))?,
    };
    audio::validate_prepared_request(&audio_request, &prepared.audio)?;
    validate_prepared(prepared)
}
pub fn validate_result(
    input: &PreparedSequenceTranscribeV4,
    result: &SequenceTranscribeResultV4,
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
    let stored = audio::StoredV4AudioWavResultV1 {
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
