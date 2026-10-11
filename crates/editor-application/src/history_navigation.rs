//! Pure reconstruction of editor stacks. New edits invalidate redo; selective operations retain causality.
use opencut_api_contracts::history_navigation::{RedoTarget, UndoTarget};
use std::collections::{BTreeMap, HashMap};

pub enum HistoryStep {
    Edit(UndoTarget),
    Undo {
        transaction_id: String,
        target: String,
    },
    Redo {
        undo_transaction_id: String,
        target: String,
    },
}

#[derive(Default)]
pub struct Navigation {
    next: usize,
    undo: BTreeMap<usize, UndoTarget>,
    redo: BTreeMap<usize, RedoTarget>,
    edits: HashMap<String, usize>,
    reversals: HashMap<String, usize>,
    labels: HashMap<String, Option<String>>,
}

impl Navigation {
    pub fn push(&mut self, step: HistoryStep) {
        let order = self.next;
        self.next += 1;
        match step {
            HistoryStep::Edit(target) => {
                self.labels
                    .insert(target.transaction_id.clone(), target.label.clone());
                self.edits.insert(target.transaction_id.clone(), order);
                self.undo.insert(order, target);
                self.redo.clear();
                self.reversals.clear();
            }
            HistoryStep::Undo {
                transaction_id,
                target,
            } => {
                if let Some(index) = self.edits.remove(&target) {
                    self.undo.remove(&index);
                }
                self.reversals.insert(transaction_id.clone(), order);
                self.redo.insert(
                    order,
                    RedoTarget {
                        label: self.labels.get(&target).cloned().flatten(),
                        transaction_id: target,
                        undo_transaction_id: transaction_id,
                    },
                );
            }
            HistoryStep::Redo {
                undo_transaction_id,
                target,
            } => {
                if let Some(index) = self.reversals.remove(&undo_transaction_id) {
                    self.redo.remove(&index);
                }
                if let Some(index) = self.edits.insert(target.clone(), order) {
                    self.undo.remove(&index);
                }
                self.undo.insert(
                    order,
                    UndoTarget {
                        label: self.labels.get(&target).cloned().flatten(),
                        transaction_id: target,
                    },
                );
            }
        }
    }
    pub fn targets(self) -> (Option<UndoTarget>, Option<RedoTarget>) {
        (
            self.undo.into_values().next_back(),
            self.redo.into_values().next_back(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edit(id: &str) -> HistoryStep {
        HistoryStep::Edit(UndoTarget {
            transaction_id: id.into(),
            label: Some(id.into()),
        })
    }
    fn undo(id: &str, target: &str) -> HistoryStep {
        HistoryStep::Undo {
            transaction_id: id.into(),
            target: target.into(),
        }
    }
    fn redo(id: &str, target: &str) -> HistoryStep {
        HistoryStep::Redo {
            undo_transaction_id: id.into(),
            target: target.into(),
        }
    }
    #[test]
    fn consecutive_navigation_and_new_edits_follow_editor_order() {
        let mut state = Navigation::default();
        for step in [
            edit("a"),
            edit("b"),
            undo("ub", "b"),
            undo("ua", "a"),
            redo("ua", "a"),
        ] {
            state.push(step);
        }
        assert_eq!(state.undo.last_key_value().unwrap().1.transaction_id, "a");
        assert_eq!(
            state.redo.last_key_value().unwrap().1.undo_transaction_id,
            "ub"
        );
        state.push(redo("ub", "b"));
        state.push(undo("ub2", "b"));
        state.push(edit("c"));
        let (undo, redo) = state.targets();
        assert_eq!(undo.unwrap().transaction_id, "c");
        assert!(redo.is_none());
    }
    #[test]
    fn selective_redo_becomes_latest_undo_without_losing_independent_edits() {
        let mut state = Navigation::default();
        for step in [
            edit("a"),
            edit("b"),
            undo("ua", "a"),
            edit("c"),
            redo("ua", "a"),
            undo("ua2", "a"),
        ] {
            state.push(step);
        }
        let (undo, redo) = state.targets();
        assert_eq!(undo.unwrap().transaction_id, "c");
        assert_eq!(redo.unwrap().undo_transaction_id, "ua2");
    }
}
