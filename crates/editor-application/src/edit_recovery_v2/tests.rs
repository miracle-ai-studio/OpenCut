use super::*;
use opencut_contracts::{
    edit_recovery_v1::{EditRecoveryContextV1, EditRecoveryRecordV1},
    project_v4_history::{HistoryCommitV4, RedoV4Project, UndoV4Project},
    project_v4_ops::RestoreV4Revision,
    project_v4_public::{ApplyPublicV4Project, PublicProjectOperationV4},
    timeline_v4::{
        TimelineCommandV4, TimelineDecisionV4, TimelineEditPlanV4, TimelineEditRequestV4,
    },
};
use std::cell::Cell;

fn requests() -> Vec<StageEditRecoveryV2> {
    let intents = vec![
        EditRecoveryIntentV2::PublicApply {
            request: Box::new(ApplyPublicV4Project {
                project_id: "p".into(),
                expected_revision: "base".into(),
                idempotency_key: "apply-key".into(),
                label: "Rename".into(),
                operations: vec![PublicProjectOperationV4::Rename { name: "New".into() }],
            }),
        },
        EditRecoveryIntentV2::Undo {
            request: UndoV4Project {
                project_id: "p".into(),
                expected_revision: "base".into(),
                target_transaction_id: "original".into(),
                idempotency_key: "undo-key".into(),
            },
        },
        EditRecoveryIntentV2::Redo {
            request: RedoV4Project {
                project_id: "p".into(),
                expected_revision: "base".into(),
                undo_transaction_id: "undo".into(),
                idempotency_key: "redo-key".into(),
            },
        },
        EditRecoveryIntentV2::Restore {
            request: RestoreV4Revision {
                project_id: "p".into(),
                expected_revision: "base".into(),
                target_revision: "older".into(),
                idempotency_key: "restore-key".into(),
                asset_versions: Default::default(),
            },
        },
    ];
    intents
        .into_iter()
        .map(|intent| StageEditRecoveryV2 {
            context: EditRecoveryContextV2 {
                intent,
                sequence_id: None,
            },
        })
        .collect()
}
fn transaction() -> TransactionResult {
    TransactionResult {
        transaction_id: "saved".into(),
        base_revision: "base".into(),
        revision: "new".into(),
        created_ids: vec!["new-element".into()],
        changed_ids: vec!["p".into()],
        deleted_ids: vec![],
    }
}
fn receipts() -> Vec<EditRecoveryReceiptV2> {
    vec![
        EditRecoveryReceiptV2::PublicApply {
            transaction: transaction(),
        },
        EditRecoveryReceiptV2::Undo {
            commit: HistoryCommitV4 {
                direction: HistoryDirectionV4::Undo,
                target_transaction_id: "original".into(),
                undo_transaction_id: None,
                transaction: transaction(),
            },
        },
        EditRecoveryReceiptV2::Redo {
            commit: HistoryCommitV4 {
                direction: HistoryDirectionV4::Redo,
                target_transaction_id: "original".into(),
                undo_transaction_id: Some("undo".into()),
                transaction: transaction(),
            },
        },
        EditRecoveryReceiptV2::Restore {
            transaction: transaction(),
        },
    ]
}
#[derive(Default)]
struct Store {
    denied: bool,
    stage_calls: usize,
    inspect_calls: Cell<usize>,
    ack_calls: usize,
    stage_override: Option<EditRecoveryRecordV2>,
    found: Option<DiscoveredEditRecoveryV2>,
    acknowledgement: Option<EditRecoveryAckV2>,
}
impl EditRecoveryStoreV2 for Store {
    fn stage_recovery_v2(
        &mut self,
        request: &StageEditRecoveryV2,
        prepared: &PreparedEditRecoveryV2,
    ) -> Result<EditRecoveryRecordV2, EditorError> {
        self.stage_calls += 1;
        if self.denied {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Original credential was revoked",
            ));
        }
        Ok(self.stage_override.clone().unwrap_or(EditRecoveryRecordV2 {
            schema_version: 2,
            recovery_id: "recovery".into(),
            project_id: "p".into(),
            request_digest: prepared.digest().into(),
            context: request.context.clone(),
            receipt: None,
        }))
    }
    fn inspect_recovery_v2(
        &self,
        _: &ReadEditRecoveryV2,
    ) -> Result<EditRecoveryInspectionV2, EditorError> {
        self.inspect_calls.set(self.inspect_calls.get() + 1);
        if self.denied {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Original credential was revoked",
            ));
        }
        Ok(EditRecoveryInspectionV2 {
            schema_version: 2,
            record: self.found.clone(),
        })
    }
    fn ack_recovery_v2(&mut self, _: &AckEditRecoveryV2) -> Result<EditRecoveryAckV2, EditorError> {
        self.ack_calls += 1;
        if self.denied {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Original credential was revoked",
            ));
        }
        Ok(self.acknowledgement.clone().unwrap())
    }
}
#[test]
fn four_families_keep_original_typed_requests_and_complete_public_replies() {
    let families = [
        "project_v4_public_apply",
        "project_v4_undo",
        "project_v4_redo",
        "project_v4_restore",
    ];
    for ((request, receipt), family) in requests().into_iter().zip(receipts()).zip(families) {
        let prepared = prepare(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<StageEditRecoveryV2>(prepared.payload()).unwrap(),
            request
        );
        assert_eq!(intent(&request.context.intent).family(), family);
        assert_eq!(intent(&request.context.intent).scope(), "p");
        validate_original_receipt(&request.context, &receipt).unwrap();
        let reply = serde_json::to_string(&receipt).unwrap();
        assert_eq!(
            serde_json::from_str::<EditRecoveryReceiptV2>(&reply).unwrap(),
            receipt
        );
        let mut moved_view = request.clone();
        moved_view.context.sequence_id = Some("scene".into());
        assert_ne!(prepare(&moved_view).unwrap().digest(), prepared.digest());
        assert_eq!(
            intent(&moved_view.context.intent)
                .identity()
                .unwrap()
                .current_digest(),
            intent(&request.context.intent)
                .identity()
                .unwrap()
                .current_digest()
        );
    }
}
#[test]
fn foreign_family_base_and_selective_history_targets_are_rejected() {
    let inputs = requests();
    let replies = receipts();
    for (index, input) in inputs.iter().enumerate() {
        for (reply_index, reply) in replies.iter().enumerate() {
            assert_eq!(
                validate_original_receipt(&input.context, reply).is_ok(),
                index == reply_index
            );
        }
    }
    let mut wrong = replies[1].clone();
    let EditRecoveryReceiptV2::Undo { commit } = &mut wrong else {
        unreachable!()
    };
    commit.target_transaction_id = "unrelated".into();
    assert!(validate_original_receipt(&inputs[1].context, &wrong).is_err());
    let mut wrong = replies[2].clone();
    let EditRecoveryReceiptV2::Redo { commit } = &mut wrong else {
        unreachable!()
    };
    commit.undo_transaction_id = Some("unrelated".into());
    assert!(validate_original_receipt(&inputs[2].context, &wrong).is_err());
    let wrong = EditRecoveryReceiptV2::PublicApply {
        transaction: TransactionResult {
            base_revision: "another-head".into(),
            ..transaction()
        },
    };
    assert!(validate_original_receipt(&inputs[0].context, &wrong).is_err());
}
#[test]
fn invalid_or_oversized_context_does_not_reach_storage_and_stage_must_echo_exact_intent() {
    let mut store = Store::default();
    let mut input = requests().remove(0);
    let EditRecoveryIntentV2::PublicApply { request } = &mut input.context.intent else {
        unreachable!()
    };
    request.operations = vec![PublicProjectOperationV4::Rename {
        name: "x".repeat(MAX_RECORD_BYTES),
    }];
    assert_eq!(
        stage(&mut store, &input).unwrap_err().code,
        "RECOVERY_QUOTA_EXCEEDED"
    );
    assert_eq!(store.stage_calls, 0);
    let mut input = requests().remove(1);
    input.context.sequence_id = Some("\n".into());
    assert_eq!(
        stage(&mut store, &input).unwrap_err().code,
        "INVALID_ARGUMENT"
    );
    assert_eq!(store.stage_calls, 0);
    let input = requests().remove(0);
    let mut record = stage(&mut store, &input).unwrap();
    record.context = requests().remove(1).context;
    store.stage_override = Some(record);
    assert_eq!(stage(&mut store, &input).unwrap_err().code, "STORAGE_ERROR");
}
#[test]
fn inspect_has_non_null_absence_and_preserves_legacy_v1_without_dispatch() {
    let mut store = Store::default();
    let read = ReadEditRecoveryV2 {
        project_id: "p".into(),
        recovery_id: None,
    };
    let empty = inspect(&store, &read).unwrap();
    assert_eq!(
        serde_json::to_value(empty).unwrap(),
        serde_json::json!({"schema_version":2,"record":null})
    );
    let request = TimelineEditRequestV4 {
        project_id: "p".into(),
        expected_revision: "base".into(),
        sequence_id: "scene".into(),
        idempotency_key: "old-key".into(),
        label: "Old timeline".into(),
        command: TimelineCommandV4::SetTrackMuted {
            track_id: "audio".into(),
            muted: true,
        },
    };
    let EditRecoveryIntentV2::PublicApply { request: mut apply } =
        requests().remove(0).context.intent
    else {
        unreachable!()
    };
    apply.idempotency_key = request.idempotency_key.clone();
    apply.label = request.label.clone();
    let decision = TimelineDecisionV4 {
        created_ids: vec![],
        changed_ids: vec!["audio".into()],
        deleted_ids: vec![],
        selection: vec![],
        ripple_shifted_ids: vec![],
        removed_transition_ids: vec![],
    };
    let old_planner_digest =
        crate::identity::request_identity("project_v4_timeline", "p", &request)
            .unwrap()
            .current_digest()
            .to_owned();
    let legacy = EditRecoveryRecordV1 {
        schema_version: 1,
        recovery_id: "legacy".into(),
        project_id: "p".into(),
        sequence_id: "scene".into(),
        request_digest: "original-raw-digest".into(),
        receipt: None,
        context: EditRecoveryContextV1::TimelineV4 {
            request,
            plan: TimelineEditPlanV4 {
                original_request_digest: old_planner_digest,
                decision,
                transaction: Some(*apply),
            },
        },
    };
    store.found = Some(DiscoveredEditRecoveryV2::LegacyV1 {
        record: Box::new(legacy.clone()),
    });
    let found = inspect(&store, &read).unwrap();
    assert_eq!(found.record, store.found);
    assert_eq!(store.stage_calls, 0);
    assert_eq!(store.ack_calls, 0);
    assert_eq!(store.inspect_calls.get(), 2);
    assert_eq!(
        inspect(
            &store,
            &ReadEditRecoveryV2 {
                recovery_id: Some("other".into()),
                ..read
            }
        )
        .unwrap_err()
        .code,
        "STORAGE_ERROR"
    );
}
#[test]
fn ack_requires_saved_receipt_and_propagates_revoked_authority_without_resolution() {
    let mut store = Store {
        denied: true,
        ..Store::default()
    };
    let input = AckEditRecoveryV2 {
        project_id: "p".into(),
        recovery_id: "recovery".into(),
    };
    assert_eq!(ack(&mut store, &input).unwrap_err().code, "NOT_AUTHORIZED");
    store.denied = false;
    store.acknowledgement = Some(EditRecoveryAckV2 {
        schema_version: 2,
        recovery_id: "recovery".into(),
        resolution: EditRecoveryResolutionV2::Saved,
        receipt: None,
    });
    assert_eq!(ack(&mut store, &input).unwrap_err().code, "STORAGE_ERROR");
    store.acknowledgement.as_mut().unwrap().receipt = Some(receipts().remove(1));
    assert_eq!(
        ack(&mut store, &input).unwrap().receipt,
        store.acknowledgement.as_ref().unwrap().receipt
    );
    store.acknowledgement.as_mut().unwrap().resolution = EditRecoveryResolutionV2::NotSaved;
    assert_eq!(ack(&mut store, &input).unwrap_err().code, "STORAGE_ERROR");
    store.acknowledgement.as_mut().unwrap().receipt = None;
    assert_eq!(
        ack(&mut store, &input).unwrap().resolution,
        EditRecoveryResolutionV2::NotSaved
    );
    assert_eq!(store.stage_calls, 0);
    assert_eq!(store.inspect_calls.get(), 0);
}
