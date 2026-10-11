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
test("current source obeys boundaries", () =>
  assert.deepEqual(inspect().errors, []));

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
