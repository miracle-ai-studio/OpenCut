import assert from "node:assert/strict";

// Behavioral verifiers still exercise their original transport. Unwrap only
// after checking the canonical response's actual view;never accept an old bare
// snapshot or manufacture a compatibility response.
export async function editorSnapshot(pending) {
  const view = await pending;
  assert.equal(view.kind, "editor", "Expected editable project content");
  assert.ok(view.snapshot?.project && view.snapshot.revision);
  return view.snapshot;
}

export async function preservedProject(pending) {
  const view = await pending;
  assert.equal(view.kind, "preserved", "Expected preserved project content");
  assert.equal(view.view?.read_only, true);
  return view.view;
}
