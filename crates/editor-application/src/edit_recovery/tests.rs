use super::*;
use opencut_contracts::{
    project_history::{HistoryCommit, RedoRequest, UndoRequest},
    project_public::{ApplyProject, ProjectEditOperation},
    scene_ops::RestoreProject,
};
use std::cell::Cell;

fn requests() -> Vec<StageEditRecovery> {
    let intents = vec![
        EditRecoveryIntent::FontImport {
            request: opencut_contracts::font_import::ImportFont {
                project_id: "p".into(),
                expected_revision: "base".into(),
                idempotency_key: "font-static".into(),
                catalog_id: "google-lato-regular-v1".into(),
                font_id: "font".into(),
                required_text: "AV".into(),
            },
        },
        EditRecoveryIntent::FontImport {
            request: opencut_contracts::font_import::ImportFont {
                project_id: "p".into(),
                expected_revision: "base".into(),
                idempotency_key: "font-derived".into(),
                catalog_id: crate::font_derived::catalog().unwrap()[0]
                    .source
                    .face
                    .id
                    .clone(),
                font_id: "font".into(),
                required_text: "AV".into(),
            },
        },
        EditRecoveryIntent::PublicApply {
            request: Box::new(ApplyProject {
                branch_name: None,
                project_id: "p".into(),
                expected_revision: "base".into(),
                idempotency_key: "apply-key".into(),
                label: "Rename".into(),
                operations: vec![ProjectEditOperation::Rename { name: "New".into() }],
            }),
        },
        EditRecoveryIntent::Undo {
            request: UndoRequest {
                project_id: "p".into(),
                expected_revision: "base".into(),
                target_transaction_id: "original".into(),
                idempotency_key: "undo-key".into(),
            },
        },
        EditRecoveryIntent::Redo {
            request: RedoRequest {
                project_id: "p".into(),
                expected_revision: "base".into(),
                undo_transaction_id: "undo".into(),
                idempotency_key: "redo-key".into(),
            },
        },
        EditRecoveryIntent::Restore {
            request: RestoreProject {
                branch_name: None,
                label: None,
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
        .map(|intent| StageEditRecovery {
            context: EditRecoveryContext {
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
fn receipts() -> Vec<EditRecoveryReceipt> {
    vec![
        EditRecoveryReceipt::FontImport {
            transaction: transaction(),
        },
        EditRecoveryReceipt::FontImport {
            transaction: transaction(),
        },
        EditRecoveryReceipt::PublicApply {
            transaction: transaction(),
        },
        EditRecoveryReceipt::Undo {
            commit: HistoryCommit {
                direction: Direction::Undo,
                target_transaction_id: "original".into(),
                undo_transaction_id: None,
                transaction: transaction(),
            },
        },
        EditRecoveryReceipt::Redo {
            commit: HistoryCommit {
                direction: Direction::Redo,
                target_transaction_id: "original".into(),
                undo_transaction_id: Some("undo".into()),
                transaction: transaction(),
            },
        },
        EditRecoveryReceipt::Restore {
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
    stage_override: Option<EditRecoveryRecord>,
    found: Option<EditRecoveryRecord>,
    acknowledgement: Option<EditRecoveryAck>,
}
impl EditRecoveryStore for Store {
    fn stage_recovery(
        &mut self,
        request: &StageEditRecovery,
        prepared: &PreparedEditRecovery,
    ) -> Result<EditRecoveryRecord, EditorError> {
        self.stage_calls += 1;
        if self.denied {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Original credential was revoked",
            ));
        }
        Ok(self.stage_override.clone().unwrap_or(EditRecoveryRecord {
            recovery_id: "recovery".into(),
            project_id: "p".into(),
            request_digest: prepared.digest().into(),
            context: request.context.clone(),
            receipt: None,
        }))
    }
    fn inspect_recovery(
        &self,
        _: &ReadEditRecovery,
    ) -> Result<EditRecoveryInspection, EditorError> {
        self.inspect_calls.set(self.inspect_calls.get() + 1);
        if self.denied {
            return Err(EditorError::new(
                "NOT_AUTHORIZED",
                "Original credential was revoked",
            ));
        }
        Ok(EditRecoveryInspection {
            record: self.found.clone(),
        })
    }
    fn ack_recovery(&mut self, _: &AckEditRecovery) -> Result<EditRecoveryAck, EditorError> {
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
fn write_families_keep_original_typed_requests_and_complete_public_replies() {
    let families = [
        "project_v4_font_import_v1",
        "project_v4_derived_font_import_v1",
        "project_v4_public_apply",
        "project_v4_undo",
        "project_v4_redo",
        "project_v4_restore",
    ];
    for ((request, receipt), family) in requests().into_iter().zip(receipts()).zip(families) {
        let prepared = prepare(&request).unwrap();
        assert_eq!(
            serde_json::from_str::<StageEditRecovery>(prepared.payload()).unwrap(),
            request
        );
        assert_eq!(intent(&request.context.intent).unwrap().family(), family);
        assert_eq!(intent(&request.context.intent).unwrap().scope(), "p");
        validate_original_receipt(&request.context, &receipt).unwrap();
        let reply = serde_json::to_string(&receipt).unwrap();
        assert_eq!(
            serde_json::from_str::<EditRecoveryReceipt>(&reply).unwrap(),
            receipt
        );
        let mut moved_view = request.clone();
        moved_view.context.sequence_id = Some("scene".into());
        assert_ne!(prepare(&moved_view).unwrap().digest(), prepared.digest());
        assert_eq!(
            intent(&moved_view.context.intent)
                .unwrap()
                .identity()
                .unwrap()
                .current_digest(),
            intent(&request.context.intent)
                .unwrap()
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
    // Both acquisition algorithms intentionally share one public receipt shape.
    // The store binds that receipt to the exact original journal request.
    for (index, input) in inputs.iter().enumerate() {
        for (reply_index, reply) in replies.iter().enumerate() {
            assert_eq!(
                validate_original_receipt(&input.context, reply).is_ok(),
                index == reply_index || (index < 2 && reply_index < 2)
            );
        }
    }
    let mut wrong = replies[3].clone();
    let EditRecoveryReceipt::Undo { commit } = &mut wrong else {
        unreachable!()
    };
    commit.target_transaction_id = "unrelated".into();
    assert!(validate_original_receipt(&inputs[3].context, &wrong).is_err());
    let mut wrong = replies[4].clone();
    let EditRecoveryReceipt::Redo { commit } = &mut wrong else {
        unreachable!()
    };
    commit.undo_transaction_id = Some("unrelated".into());
    assert!(validate_original_receipt(&inputs[4].context, &wrong).is_err());
    let wrong = EditRecoveryReceipt::PublicApply {
        transaction: TransactionResult {
            base_revision: "another-head".into(),
            ..transaction()
        },
    };
    assert!(validate_original_receipt(&inputs[2].context, &wrong).is_err());
}
#[test]
fn invalid_or_oversized_context_does_not_reach_storage_and_stage_must_echo_exact_intent() {
    let mut store = Store::default();
    let mut input = requests().remove(2);
    let EditRecoveryIntent::PublicApply { request } = &mut input.context.intent else {
        unreachable!()
    };
    request.operations = vec![ProjectEditOperation::Rename {
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
    let input = requests().remove(2);
    let mut record = stage(&mut store, &input).unwrap();
    record.context = requests().remove(1).context;
    store.stage_override = Some(record);
    assert_eq!(stage(&mut store, &input).unwrap_err().code, "STORAGE_ERROR");
}
#[test]
fn inspect_has_explicit_absence_and_preserves_exact_record_without_dispatch() {
    let mut store = Store::default();
    let read = ReadEditRecovery {
        project_id: "p".into(),
        recovery_id: None,
    };
    assert_eq!(
        serde_json::to_value(inspect(&store, &read).unwrap()).unwrap(),
        serde_json::json!({"record":null})
    );
    let input = requests().remove(2);
    let prepared = prepare(&input).unwrap();
    store.found = Some(EditRecoveryRecord {
        recovery_id: "original".into(),
        project_id: "p".into(),
        request_digest: prepared.digest().into(),
        context: input.context,
        receipt: None,
    });
    assert_eq!(inspect(&store, &read).unwrap().record, store.found);
    assert_eq!(store.stage_calls, 0);
    assert_eq!(store.ack_calls, 0);
    assert_eq!(
        inspect(
            &store,
            &ReadEditRecovery {
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
    let input = AckEditRecovery {
        project_id: "p".into(),
        recovery_id: "recovery".into(),
    };
    assert_eq!(ack(&mut store, &input).unwrap_err().code, "NOT_AUTHORIZED");
    store.denied = false;
    store.acknowledgement = Some(EditRecoveryAck {
        recovery_id: "recovery".into(),
        resolution: EditRecoveryResolution::Saved,
        receipt: None,
    });
    assert_eq!(ack(&mut store, &input).unwrap_err().code, "STORAGE_ERROR");
    store.acknowledgement.as_mut().unwrap().receipt = Some(receipts().remove(3));
    assert_eq!(
        ack(&mut store, &input).unwrap().receipt,
        store.acknowledgement.as_ref().unwrap().receipt
    );
    store.acknowledgement.as_mut().unwrap().resolution = EditRecoveryResolution::NotSaved;
    assert_eq!(ack(&mut store, &input).unwrap_err().code, "STORAGE_ERROR");
    store.acknowledgement.as_mut().unwrap().receipt = None;
    assert_eq!(
        ack(&mut store, &input).unwrap().resolution,
        EditRecoveryResolution::NotSaved
    );
    assert_eq!(store.stage_calls, 0);
    assert_eq!(store.inspect_calls.get(), 0);
}
