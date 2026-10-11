//! Pure policy for the versioned package verification view.
use opencut_api_contracts::reproducibility::{
    ExecutionIdentityV1, InspectV1, ReportV1, VerificationStatus,
};
use opencut_contracts::EditorError;

pub struct PackageObservation {
    pub manifest_sha256: String,
    pub source_revision: String,
    pub package_schema_version: u32,
    pub legacy_claim: bool,
    pub media_self_contained_declared: bool,
    pub native_tools_match: bool,
    pub current_execution: ExecutionIdentityV1,
}

pub trait ReproducibilityProbe {
    fn observe(&self, input: &InspectV1) -> Result<PackageObservation, EditorError>;
}

/// Supported legacy packages do not identify their original OpenCut engine.
pub fn inspect(
    input: &InspectV1,
    caller: &crate::access::Caller,
    probe: &impl ReproducibilityProbe,
) -> Result<ReportV1, EditorError> {
    if !caller.is_owner() {
        return Err(crate::access::denied());
    }
    if input.expected_manifest_sha256.len() != 64
        || !input
            .expected_manifest_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(EditorError::new(
            "INVALID_ARGUMENT",
            "Expected a lowercase manifest SHA-256",
        ));
    }
    let observed = probe.observe(input)?;
    if observed.manifest_sha256 != input.expected_manifest_sha256
        || !(2..=5).contains(&observed.package_schema_version)
    {
        return Err(EditorError::new(
            "PACKAGE_INVALID",
            "Observation does not match the requested legacy package",
        ));
    }
    Ok(ReportV1 {
        schema_version: 1,
        manifest_sha256: observed.manifest_sha256,
        source_revision: observed.source_revision,
        package_schema_version: observed.package_schema_version,
        legacy_fully_reproducible_claim: observed.legacy_claim,
        media_self_contained_declared: observed.media_self_contained_declared,
        media_contents_verified: VerificationStatus::NotVerified,
        native_tool_environment: if observed.package_schema_version == 2 {
            VerificationStatus::NotVerified
        } else if observed.native_tools_match {
            VerificationStatus::Verified
        } else {
            VerificationStatus::Mismatch
        },
        execution_environment_locked: VerificationStatus::NotVerified,
        semantic_output_verified: VerificationStatus::NotVerified,
        bit_exact_verified: VerificationStatus::NotVerified,
        strictly_reproducible: false,
        current_execution: observed.current_execution,
        reasons: vec![
            "original_opencut_engine_identity_missing".into(),
            "media_and_plugin_bytes_not_verified_by_this_inspection".into(),
            "no_independent_output_verification_receipt".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FakeProbe(u32);
    impl ReproducibilityProbe for FakeProbe {
        fn observe(&self, input: &InspectV1) -> Result<PackageObservation, EditorError> {
            Ok(PackageObservation {
                manifest_sha256: input.expected_manifest_sha256.clone(),
                source_revision: "rev".into(),
                package_schema_version: self.0,
                legacy_claim: self.0 >= 3,
                media_self_contained_declared: true,
                native_tools_match: self.0 >= 3,
                current_execution: ExecutionIdentityV1 {
                    process_binary_sha256: "b".repeat(64),
                    platform: "fake".into(),
                    native_tools: vec![],
                },
            })
        }
    }
    #[test]
    fn legacy_true_and_matching_tools_do_not_prove_reproducibility() {
        let report = inspect(
            &InspectV1 {
                package_path: "opaque-location".into(),
                expected_manifest_sha256: "a".repeat(64),
            },
            &crate::access::Caller::LocalOwner,
            &FakeProbe(5),
        )
        .unwrap();
        assert!(report.legacy_fully_reproducible_claim);
        assert_eq!(report.native_tool_environment, VerificationStatus::Verified);
        assert_eq!(
            report.execution_environment_locked,
            VerificationStatus::NotVerified
        );
        assert!(!report.strictly_reproducible);
    }
    #[test]
    fn legacy_unlocked_tools_are_unknown_rather_than_a_mismatch() {
        let report = inspect(
            &InspectV1 {
                package_path: "opaque".into(),
                expected_manifest_sha256: "a".repeat(64),
            },
            &crate::access::Caller::LocalOwner,
            &FakeProbe(2),
        )
        .unwrap();
        assert_eq!(
            report.native_tool_environment,
            VerificationStatus::NotVerified
        );
        assert!(!report.strictly_reproducible);
    }

    #[test]
    fn invalid_identity_rejected_before_native_observation() {
        assert_eq!(
            inspect(
                &InspectV1 {
                    package_path: "opaque-location".into(),
                    expected_manifest_sha256: "invalid".into()
                },
                &crate::access::Caller::LocalOwner,
                &FakeProbe(5)
            )
            .unwrap_err()
            .code,
            "INVALID_ARGUMENT"
        );
    }

    #[test]
    fn restricted_caller_cannot_trigger_native_reads() {
        struct MustNotRun;
        impl ReproducibilityProbe for MustNotRun {
            fn observe(&self, _: &InspectV1) -> Result<PackageObservation, EditorError> {
                panic!("unauthorized native read");
            }
        }
        let caller = crate::access::Caller::Credential(
            crate::access::VerifiedCredential::from_verified("reader".into(), "credential".into()),
        );
        assert_eq!(
            inspect(
                &InspectV1 {
                    package_path: "opaque".into(),
                    expected_manifest_sha256: "a".repeat(64)
                },
                &caller,
                &MustNotRun
            )
            .unwrap_err()
            .code,
            "NOT_AUTHORIZED"
        );
    }
}
