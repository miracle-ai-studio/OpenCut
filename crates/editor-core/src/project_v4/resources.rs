//! Pure V4 resources rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn background(value: &BackgroundV4) -> Result<(), EditorError> {
    match value {
        BackgroundV4::Color { color: value } => color(value),
        BackgroundV4::MainTrackBlur { intensity } => {
            nonnegative(*intensity, "Classic background blur")
        }
    }
}

pub(super) fn origin(value: &ResourceOriginV4) -> Result<(), EditorError> {
    match value {
        ResourceOriginV4::LocalImport { original_name } => {
            required(original_name, "Original resource name")
        }
        ResourceOriginV4::Online {
            provider,
            source_url,
        } => {
            required(provider, "Resource provider")?;
            required(source_url, "Resource source URL")
        }
        ResourceOriginV4::LegacyLocator { uri } => required(uri, "Legacy provenance locator"),
        ResourceOriginV4::Generated {
            provider,
            generation_id,
        } => {
            required(provider, "Generation provider")?;
            required(generation_id, "Generation identity")
        }
    }
}

/// Returns the declared amplitude without changing its stored interpolation unit.
pub fn gain_amplitude(value: &GainV4) -> Result<f64, EditorError> {
    match *value {
        GainV4::LinearLegacy { value } => {
            bound(value, 0.0, 4.0, "Legacy gain")?;
            Ok(value)
        }
        GainV4::Decibels { value } => {
            finite(value, "Gain dB")?;
            let amplitude = 10.0_f64.powf(value / 20.0);
            finite(amplitude, "Gain amplitude")?;
            Ok(amplitude)
        }
    }
}
