//! Private fixed-runtime Node probe; the locked Mediabunny library owns packet statistics.
use crate::{CancellationToken, MediaError, MediaResult, OutputAdmission, process};
use opencut_media_contracts::ClassicPacketRate;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct ClassicPacketRateConfig {
    pub node: PathBuf,
    pub helper: PathBuf,
    pub bundle: PathBuf,
    pub expected_helper_sha256: String,
    pub expected_bundle_sha256: String,
}

fn fixed_file(path: &Path) -> MediaResult<()> {
    if !path.is_absolute() || !path.is_file() {
        return Err(MediaError::InvalidRequest(
            "Packet-rate runtime and input must be fixed absolute files".into(),
        ));
    }
    Ok(())
}

fn verify_module(path: &Path, expected: &str, cancel: &CancellationToken) -> MediaResult<()> {
    fixed_file(path)?;
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(MediaError::InvalidRequest(
            "Invalid packet-rate module hash".into(),
        ));
    }
    let mut file = File::open(path)?;
    if !(1..=8 * 1024 * 1024).contains(&file.metadata()?.len()) {
        return Err(MediaError::InvalidRequest(
            "Packet-rate module exceeds its fixed budget".into(),
        ));
    }
    let mut hash = Sha256::new();
    let mut bytes = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let count = file.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > 8 * 1024 * 1024 {
            return Err(MediaError::InvalidRequest(
                "Packet-rate module exceeds its fixed budget".into(),
            ));
        }
        hash.update(&bytes[..count]);
    }
    if format!("{:x}", hash.finalize()) != expected {
        return Err(MediaError::VerificationFailed(
            "Packet-rate module identity changed".into(),
        ));
    }
    Ok(())
}

pub fn run_classic_packet_rate(
    config: &ClassicPacketRateConfig,
    input: &Path,
    cancel: &CancellationToken,
    admission: Option<&dyn OutputAdmission>,
) -> MediaResult<ClassicPacketRate> {
    if cancel.is_cancelled() {
        return Err(MediaError::Cancelled);
    }
    fixed_file(&config.node)?;
    fixed_file(input)?;
    verify_module(&config.helper, &config.expected_helper_sha256, cancel)?;
    verify_module(&config.bundle, &config.expected_bundle_sha256, cancel)?;
    let bytes = process::checked_output_limited(
        Command::new(&config.node)
            .arg("--max-old-space-size=256")
            .arg(&config.helper)
            .arg(&config.bundle)
            .arg(&config.expected_bundle_sha256)
            .arg(input)
            .env_remove("NODE_OPTIONS")
            .env_remove("NODE_PATH"),
        "classic-packet-rate",
        cancel,
        Duration::from_secs(30),
        admission,
    )?;
    if cancel.is_cancelled() {
        return Err(MediaError::Cancelled);
    }
    let observation: ClassicPacketRate =
        serde_json::from_slice(&bytes).map_err(|_| MediaError::InvalidToolOutput {
            tool: "classic-packet-rate".into(),
            detail: "Invalid typed packet-rate observation".into(),
        })?;
    observation
        .validate()
        .map_err(|detail| MediaError::InvalidToolOutput {
            tool: "classic-packet-rate".into(),
            detail,
        })?;
    Ok(observation)
}
