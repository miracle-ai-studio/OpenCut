import { test } from "node:test";
import assert from "node:assert/strict";
import { violations, inspect } from "./check-boundaries.mjs";
test("allows features to share pure requests and injected state", () =>
  assert.deepEqual(
    violations(
      "editor/timeline/panel.tsx",
      'import {placeAsset} from "../model/edit-requests"; import {useWorkspace} from "../workspace/context";',
    ),
    [],
  ));
test("rejects direct feature coupling", () =>
  assert.equal(
    violations(
      "editor/timeline/panel.tsx",
      'import {upload} from "../assets/upload";',
    ).length,
    1,
  ));
test("rejects reverse state and model dependencies", () => {
  assert.equal(
    violations("editor/model/time.ts", 'import {api} from "../workspace/api";')
      .length,
    1,
  );
  assert.equal(
    violations(
      "editor/workspace/controller.ts",
      'import {Panel} from "../timeline/panel";',
    ).length,
    1,
  );
});
test("rejects a second browser project authority", () =>
  assert.equal(
    violations("editor/projects/store.ts", 'indexedDB.open("projects")').length,
    1,
  ));
test("recovery locators have one bounded session adapter and no transaction storage exception", () => {
  const file = "editor/workspace/v4/timeline-edit/recovery.ts";
  assert.deepEqual(violations(file, 'sessionStorage.getItem("opencut.edit-recovery.v1"); sessionStorage.setItem("opencut.edit-recovery.v1",payload);'), []);
  for (const code of [
    'sessionStorage.setItem("project",payload);',
    'const store=sessionStorage; store.setItem("opencut.edit-recovery.v1",payload);',
    'sessionStorage["setItem"]("opencut.edit-recovery.v1",payload);',
    'localStorage.setItem("opencut.edit-recovery.v1",payload);',
  ]) assert.equal(violations(file, code).length, 1, code);
  assert.equal(violations("editor/workspace/controller.ts", 'sessionStorage.setItem("opencut.edit-recovery.v1",payload);').length, 1);
});
test("current source obeys boundaries", () =>
  assert.deepEqual(inspect().errors, []));
test("memory test fixtures are excluded from runtime scanning and cannot be runtime dependencies", () => {
  assert.equal(violations("editor/workspace/controller.ts", 'import { setupV4 } from "./v4/fixture";').length, 1);
  assert.equal(violations("editor/workspace/v4/session.ts", 'import { setup } from "./timeline-edit/timeline-fixture";').length, 1);
  assert.deepEqual(violations("editor/workspace/v4/session.test.ts", 'import { setupV4 } from "./fixture";'), []);
});
test("V2 recovery permits only its reviewed locator key in its own adapter", () => {
  const file = "editor/workspace/v4/edit-recovery-pointers.ts";
  assert.deepEqual(violations(file, 'sessionStorage.getItem("opencut.edit-recovery.v2"); sessionStorage.setItem("opencut.edit-recovery.v2",payload);'), []);
  for (const code of [
    'sessionStorage.setItem("opencut.edit-recovery.v1",payload);',
    'localStorage.setItem("opencut.edit-recovery.v2",payload);',
    'const store=sessionStorage; store.setItem("opencut.edit-recovery.v2",payload);',
  ]) assert.equal(violations(file, code).length, 1);
  assert.equal(violations("editor/workspace/controller.ts", 'sessionStorage.setItem("opencut.edit-recovery.v2",payload);').length, 1);
});
test("audio output shares only the pure mastering bridge, never the renderer host", () => {
  const path = "../../../../../../packages/classic-audio-renderer/src/mastering.mjs";
  assert.deepEqual(violations("editor/workspace/v4/audio-preview-mastering.ts", `export {createClassicMasteringChain} from "${path}";`), []);
  for (const [file, name] of [
    ["editor/workspace/v4/audio-preview.ts", path],
    ["editor/workspace/v4/audio-preview-mastering.ts", "@opencut/classic-audio-renderer"],
    ["editor/workspace/v4/audio-preview-mastering.ts", "../../../../../../packages/classic-audio-renderer/host/runner.mjs"],
  ]) assert.equal(violations(file, `import x from "${name}";`).length, 1);
});
test("legacy browser input adapter is readonly and cannot acquire modern persistence authority", () => {
  const file = "editor/workspace/classic/source.ts";
  assert.deepEqual(
    violations(
      file,
      'const open=indexedDB.open(name); open.transaction?.abort(); db.transaction("projects","readonly"); root.getDirectoryHandle(name); dir.getFileHandle(id);',
    ),
    [],
  );
  for (const code of [
    "indexedDB.open(name, 2);",
    "indexedDB.deleteDatabase(name);",
    'indexedDB.open(name); db.transaction("projects","readwrite");',
    "indexedDB.open(name); store.put(data);",
    'indexedDB.open(name); store["put"](data);',
    'indexedDB.open(name); const method="clear"; store[method]();',
    "indexedDB.open(name); const write=store.put; write(data);",
    'indexedDB.open(name); const tx=db.transaction; tx("projects","readwrite");',
    "indexedDB.open(name); store.delete(id);",
    "indexedDB.open(name); dir.getFileHandle(id,{create:true});",
    'indexedDB.open(name); localStorage.setItem("project",data);',
  ])
    assert.equal(violations(file, code).length, 1, code);
  assert.equal(
    violations("editor/projects/reader.ts", "indexedDB.open(name);").length,
    1,
  );
});

test("rejects aliases, require and type-only cross-feature imports", () => {
  for (const code of [
    'import {x} from "@/editor/assets/upload";',
    'export * from "#/editor/assets/upload";',
    'const x=require("../assets/upload");',
    'type X=import("../assets/upload").X;',
  ])
    assert.equal(violations("editor/timeline/panel.tsx", code).length, 1);
});
test("rejects runtime require lookup", () =>
  assert.equal(
    violations("editor/timeline/panel.tsx", "const x=require(moduleName);")
      .length,
    1,
  ));
test("UI view preference storage cannot become project or credential storage", () => {
  assert.deepEqual(
    violations(
      "editor/shared/view-preferences.ts",
      'localStorage.getItem("opencut.cover-scenes.v1");',
    ),
    [],
  );
  assert.equal(
    violations(
      "editor/projects/cover.tsx",
      'localStorage.getItem("opencut.cover-scenes.v1");',
    ).length,
    1,
  );
  assert.deepEqual(
    violations(
      "editor/shared/view-preferences.ts",
      'localStorage.setItem("opencut.library", JSON.stringify({layout:"list",sort:"name",ascending:true}));',
    ),
    [],
  );
  assert.deepEqual(
    violations(
      "editor/shared/view-preferences.ts",
      'localStorage.getItem("opencut.timeline");',
    ),
    [],
  );
  assert.deepEqual(
    violations(
      "editor/shared/view-preferences.ts",
      'localStorage.getItem("opencut.history-views.v1");',
    ),
    [],
  );
  assert.equal(
    violations(
      "editor/workspace/controller.ts",
      'localStorage.getItem("opencut.history-views.v1");',
    ).length,
    1,
  );
  for (const code of [
    'localStorage.setItem("token", secret);',
    "const storage=localStorage;",
    "localStorage.getItem(key);",
    'indexedDB.open("projects");',
  ])
    assert.equal(
      violations("editor/shared/view-preferences.ts", code).length,
      1,
    );
});

test("public content and editing authority remain separate", () => {
  assert.ok(
    violations(
      "site/public-pages.tsx",
      'import { WorkspaceController } from "../editor/workspace/controller";',
    ).length,
  );
  assert.ok(
    violations(
      "site/public-data.ts",
      'import { EditorApi } from "../lib/editor-sdk";',
    ).length,
  );
  assert.ok(
    violations(
      "editor/workspace/controller.ts",
      'import { posts } from "../../site/public-data";',
    ).length,
  );
  assert.equal(
    violations(
      "site/site.tsx",
      'import { useLocale } from "../editor/shared/locale";',
    ).length,
    0,
  );
});

test("feedback adapter may persist only its two bounded personal-data keys", () => {
  const file = "editor/feedback/mock-receiver.ts";
  assert.deepEqual(
    violations(
      file,
      'localStorage.getItem("opencut.feedback.draft.v1"); localStorage.setItem("opencut.feedback.mock-history.v1", data);',
    ),
    [],
  );
  for (const code of [
    'localStorage.setItem("project", data);',
    'localStorage.setItem("token", secret);',
    "localStorage.getItem(key);",
    "localStorage.clear();",
    'localStorage.removeItem("opencut.feedback.draft.v1");',
    "const storage = localStorage;",
    'localStorage["setItem"]("opencut.feedback.draft.v1", data);',
    'indexedDB.open("project");',
  ])
    assert.equal(violations(file, code).length, 1, code);
  assert.equal(
    violations(
      "editor/feedback/dialog.tsx",
      'localStorage.getItem("opencut.feedback.draft.v1");',
    ).length,
    1,
  );
  assert.equal(
    violations(
      "editor/projects/browser.tsx",
      'localStorage.getItem("opencut.feedback.mock-history.v1");',
    ).length,
    1,
  );
  assert.equal(
    violations(
      "editor/feedback/dialog.tsx",
      'import {x} from "../projects/browser";',
    ).length,
    1,
  );
  assert.equal(
    violations(
      "editor/workspace/controller.ts",
      'import {x} from "../feedback/dialog";',
    ).length,
    1,
  );
});
test("visual preview imports only the bounded execution bridge", () => {
  const path="../../../../../../packages/classic-visual-renderer/src/index.mjs";
  assert.deepEqual(violations("editor/workspace/v4/visual-preview-renderer.ts", `import {renderClassicFrame} from "${path}";`), []);
  for(const [file,name] of [["editor/workspace/v4/session.ts",path],["editor/workspace/v4/visual-preview-renderer.ts","@opencut/classic-visual-renderer"],["editor/workspace/v4/visual-preview-renderer.ts","../../../../../../packages/classic-visual-renderer/host/runner.mjs"]])
    assert.equal(violations(file, `import x from "${name}";`).length,1);
});
