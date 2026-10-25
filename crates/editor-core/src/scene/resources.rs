//! Pure V4 resources rules. No I/O or renderer capability claims.
use super::*;

pub(super) fn background(value: &Background) -> Result<(), EditorError> {
    match value {
        Background::Color { color: value } => color(value),
        Background::MainTrackBlur { intensity } => {
            nonnegative(*intensity, "Classic background blur")
        }
    }
}

pub(super) fn origin(value: &ResourceOrigin) -> Result<(), EditorError> {
    match value {
        ResourceOrigin::LocalImport { original_name } => {
            required(original_name, "Original resource name")
        }
        ResourceOrigin::Online {
            provider,
            source_url,
        } => {
            required(provider, "Resource provider")?;
            required(source_url, "Resource source URL")
        }
        ResourceOrigin::LegacyLocator { uri } => required(uri, "Legacy provenance locator"),
        ResourceOrigin::Generated {
            provider,
            generation_id,
        } => {
            required(provider, "Generation provider")?;
            required(generation_id, "Generation identity")
        }
    }
}

/// Returns the declared amplitude without changing its stored interpolation unit.
pub fn gain_amplitude(value: &Gain) -> Result<f64, EditorError> {
    match *value {
        Gain::LinearLegacy { value } => {
            bound(value, 0.0, 4.0, "Legacy gain")?;
            Ok(value)
        }
        Gain::Decibels { value } => {
            finite(value, "Gain dB")?;
            let amplitude = 10.0_f64.powf(value / 20.0);
            finite(amplitude, "Gain amplitude")?;
            Ok(amplitude)
        }
    }
}
