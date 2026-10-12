//! Fixed-revision Classic waveform display. The source cache continues to own
//! only verified raw peaks; viewport and element gain never enter its key.
use crate::source_art_v1::{
    AuthorizedSourceArtV1, DecodedSourceArtV1, PreparedSourceArtV1, SourceArtReadV1,
    prepare_source_art, validate_decoded_source_art,
};
use opencut_contracts::{
    EditorError, project_v4::*, source_art_v1::*, source_art_waveform_display_v1::*,
};
use opencut_job_execution::project_v4_audio::TrustedJobAuthorityV1;
use opencut_time::RationalTime;

mod projection;
#[cfg(test)]
mod tests;

pub const CLASSIC_WAVEFORM_DISPLAY_PROFILE_V1: &str = "classic-waveform-display-v1";
pub const CLASSIC_WAVEFORM_GAIN_SAMPLES_V1: usize = 200;
pub const MAX_WAVEFORM_DISPLAY_BARS_V1: usize = 4096;
pub const MAX_WAVEFORM_CANVAS_PIXELS_V1: u64 = 32 * 1024 * 1024 / 4;

fn invalid(message: &str) -> EditorError {
    EditorError::new("INVALID_ARGUMENT", message)
}
fn budget(message: &str) -> EditorError {
    EditorError::new("SOURCE_ART_BUDGET_EXCEEDED", message)
}
fn overflow() -> EditorError {
    EditorError::new("TIME_OVERFLOW", "Waveform display clock overflow")
}
fn unsupported(message: &str) -> EditorError {
    EditorError::new("UNSUPPORTED_WAVEFORM_DISPLAY_PROFILE", message)
}

#[derive(Clone, Debug)]
pub struct PreparedSourceArtWaveformDisplayV1 {
    request: SourceArtWaveformDisplayRequestV1,
    source_art: PreparedSourceArtV1,
    element: TimelineElementV4,
    source_start_seconds: f64,
    effective_rate: f64,
    duration_seconds: f64,
}
impl PreparedSourceArtWaveformDisplayV1 {
    pub fn request(&self) -> &SourceArtWaveformDisplayRequestV1 {
        &self.request
    }
    pub fn source_art(&self) -> &PreparedSourceArtV1 {
        &self.source_art
    }
}

fn raw_request(input: &SourceArtWaveformDisplayRequestV1) -> SourceArtRequestV1 {
    SourceArtRequestV1 {
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        asset_id: input.asset_id.clone(),
        request_id: input.request_id.clone(),
        query: SourceArtQueryV1::Waveform {
            sample_rate: input.sample_rate,
            projection: WaveformProjectionV1::Source {
                range: None,
                bins: 1,
            },
        },
    }
}

pub fn validate_request(input: &SourceArtWaveformDisplayRequestV1) -> Result<(), EditorError> {
    crate::source_art_v1::validate_request(&raw_request(input))?;
    for id in [&input.sequence_id, &input.track_id, &input.element_id] {
        if id.trim().is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
            return Err(invalid("Waveform display needs bounded element scope"));
        }
    }
    if input
        .temporary_gain
        .is_some_and(|gain| !(-600..=200).contains(&gain))
    {
        return Err(invalid("Temporary gain requires -600..200 tenths dB"));
    }
    projection::layout(&input.viewport)?;
    Ok(())
}

// Reuse the already authorized snapshot rather than asking a store to read a
// second possibly different authority/policy. Revalidation delegates to the
// original port and original trusted authority, including before prepare ends.
struct CapturedRead<'a, T> {
    store: &'a T,
    captured: &'a AuthorizedSourceArtV1,
}
impl<T: SourceArtReadV1> SourceArtReadV1 for CapturedRead<'_, T> {
    fn fixed_source_art_snapshot(
        &self,
        project_id: &str,
        revision: &str,
    ) -> Result<AuthorizedSourceArtV1, EditorError> {
        if self.captured.snapshot.project.id != project_id
            || self.captured.snapshot.revision != revision
        {
            return Err(EditorError::new(
                "STORAGE_ERROR",
                "Waveform display fixed scope differs",
            ));
        }
        Ok(self.captured.clone())
    }
    fn revalidate_source_art(
        &self,
        project_id: &str,
        revision: &str,
        authority: &TrustedJobAuthorityV1,
    ) -> Result<(), EditorError> {
        self.store
            .revalidate_source_art(project_id, revision, authority)
    }
}

pub fn prepare_waveform_display(
    store: &impl SourceArtReadV1,
    input: &SourceArtWaveformDisplayRequestV1,
) -> Result<PreparedSourceArtWaveformDisplayV1, EditorError> {
    validate_request(input)?;
    let captured = store.fixed_source_art_snapshot(&input.project_id, &input.revision)?;
    let source_art = prepare_source_art(
        &CapturedRead {
            store,
            captured: &captured,
        },
        &raw_request(input),
    )?;
    let track = captured
        .snapshot
        .project
        .sequences
        .iter()
        .find(|sequence| sequence.id == input.sequence_id)
        .and_then(|sequence| {
            sequence
                .tracks
                .iter()
                .find(|track| track.id == input.track_id)
        })
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Waveform display track is unavailable"))?;
    let element = track
        .elements
        .iter()
        .find(|element| element.id == input.element_id)
        .ok_or_else(|| EditorError::new("NOT_FOUND", "Waveform display element is unavailable"))?;
    let ElementContentV4::Audio {
        asset_id,
        audio,
        retime,
        ..
    } = &element.content
    else {
        return Err(unsupported(
            "Classic waveform display is attached to audio elements",
        ));
    };
    if asset_id != &input.asset_id {
        return Err(invalid(
            "Waveform display resource differs from its fixed element",
        ));
    }
    if !matches!(audio.gain, GainV4::Decibels { .. }) {
        return Err(unsupported(
            "Classic waveform display cannot reinterpret legacy linear gain",
        ));
    }
    if let Some(tenths) = input.temporary_gain {
        opencut_composition::project_v4::validate_preview_gain_override_v4(track, element, tenths)
            .map_err(|err| EditorError::new(err.code, err.message))?;
    }
    let source = opencut_editor_core::project_v4::resolve_element_source_request_v4(
        element,
        Some(source_art.asset()),
    )?;
    let rate = opencut_composition::project_v4::classic_effective_rate_v4(retime.rate)
        .map_err(|err| EditorError::new(err.code, err.message))?;
    let seconds = |time: &opencut_contracts::Time| -> Result<f64, EditorError> {
        let time = RationalTime::try_from(time)?;
        Ok(time.ticks() as f64 / f64::from(time.timebase()))
    };
    let source_start_seconds = seconds(&source.start)?;
    let duration_seconds = seconds(&element.timeline_range.duration)?;
    if !source_start_seconds.is_finite()
        || source_start_seconds < 0.0
        || !duration_seconds.is_finite()
        || duration_seconds <= 0.0
    {
        return Err(overflow());
    }
    store.revalidate_source_art(&input.project_id, &input.revision, source_art.authority())?;
    Ok(PreparedSourceArtWaveformDisplayV1 {
        request: input.clone(),
        source_art,
        element: element.clone(),
        source_start_seconds,
        effective_rate: f64::from(rate.numerator) / f64::from(rate.denominator),
        duration_seconds,
    })
}

pub fn project_waveform_display(
    prepared: &PreparedSourceArtWaveformDisplayV1,
    decoded: &DecodedSourceArtV1,
) -> Result<SourceArtWaveformDisplayResponseV1, EditorError> {
    validate_decoded_source_art(&prepared.source_art, decoded)?;
    let DecodedSourceArtV1::Waveform {
        sample_rate,
        channels,
        total_samples,
        bucket_size,
        amplitudes,
        ..
    } = decoded
    else {
        return Err(EditorError::new(
            "ARTIFACT_VERIFICATION_FAILED",
            "Waveform display requires source peaks",
        ));
    };
    let (canvas, bars) = projection::project(prepared, *sample_rate, *total_samples, amplitudes)?;
    let input = &prepared.request;
    Ok(SourceArtWaveformDisplayResponseV1 {
        schema_version: 1,
        project_id: input.project_id.clone(),
        revision: input.revision.clone(),
        asset_id: input.asset_id.clone(),
        sequence_id: input.sequence_id.clone(),
        track_id: input.track_id.clone(),
        element_id: input.element_id.clone(),
        version: decoded.version().clone(),
        identity: decoded.identity().clone(),
        display_profile: CLASSIC_WAVEFORM_DISPLAY_PROFILE_V1.into(),
        sample_rate: *sample_rate,
        channels: *channels,
        total_samples: total_samples.to_string(),
        bucket_size: *bucket_size,
        viewport: input.viewport.clone(),
        temporary_gain: input.temporary_gain,
        canvas,
        bars,
    })
}
