import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
const root = path.resolve(import.meta.dirname, "../src");
const features = new Set([
  "assets",
  "effects",
  "feedback",
  "connection",
  "history",
  "inspector",
  "preview",
  "projects",
  "tasks",
  "timeline",
]);
// Match the SDK's reviewed test-source classification. Fixtures may inject
// memory ports, but runtime modules cannot import them through this boundary.
const testSource = (file) => /\.(?:test|spec)(?:\.[cm]?[jt]sx?)?$/.test(file)
  || /(?:^|\/)(?:fixture|[^/]+-fixture)(?:\.[cm]?[jt]sx?)?$/.test(file);
export function violations(file, code) {
  const errors = [],
    source = ts.createSourceFile(file, code, ts.ScriptTarget.Latest, true);
  const owner = file.split("/")[1];
  const check = (name) => {
    if (typeof name !== "string") {
      errors.push(`${file}: dynamic module path`);
      return;
    }
    if (
      name.includes("classic-audio-renderer") &&
      !(
        file === "editor/workspace/v4/audio-preview-mastering.ts" &&
        name === "../../../../../../packages/classic-audio-renderer/src/mastering.mjs"
      )
    )
      errors.push(`${file}: renderer imports require the pure mastering bridge`);
    if (!name.startsWith(".") && !/^(?:@|#)\//.test(name)) return;
    const target = path.posix.normalize(
      /^(?:@|#)\//.test(name)
        ? name.slice(2)
        : path.posix.join(path.posix.dirname(file), name),
    );
    const dependency = target.split("/")[1];
    if (!testSource(file) && testSource(/\.[cm]?[jt]sx?$/.test(target) ? target : `${target}.ts`))
      errors.push(`${file}: runtime imports a test fixture`);
    if (
      file.startsWith("site/") &&
      ((target.startsWith("editor/") && !target.startsWith("editor/shared/")) ||
        target.startsWith("lib/"))
    )
      errors.push(`${file}: public site imports editing authority`);
    if (file.startsWith("editor/") && target.startsWith("site/"))
      errors.push(`${file}: editing module imports public content adapter`);
    if (features.has(owner) && features.has(dependency) && owner !== dependency)
      errors.push(`${file}: feature imports ${dependency}`);
    if (
      ["model", "shared"].includes(owner) &&
      target.startsWith("editor/") &&
      !["model", "shared"].includes(dependency)
    )
      errors.push(`${file}: foundational module imports ${dependency}`);
    if (owner === "workspace" && features.has(dependency))
      errors.push(`${file}: state imports a UI feature`);
  };
  function visit(node) {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteralLike(node.moduleSpecifier)
    )
      check(node.moduleSpecifier.text);
    if (
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) &&
          node.expression.text === "require"))
    ) {
      const argument = node.arguments[0];
      if (argument && ts.isStringLiteralLike(argument)) check(argument.text);
      else errors.push(`${file}: dynamic module path`);
    }
    if (ts.isImportTypeNode(node))
      check(
        ts.isLiteralTypeNode(node.argument) &&
          ts.isStringLiteralLike(node.argument.literal)
          ? node.argument.literal.text
          : undefined,
      );
    if (
      ts.isImportEqualsDeclaration(node) &&
      ts.isExternalModuleReference(node.moduleReference)
    )
      check(
        ts.isStringLiteralLike(node.moduleReference.expression)
          ? node.moduleReference.expression.text
          : undefined,
      );
    ts.forEachChild(node, visit);
  }
  visit(source);
  const viewPreferences =
    file === "editor/shared/view-preferences.ts" &&
    !/\b(?:indexedDB|EditorCore|sessionStorage)\b/.test(code);
  let storageCalls = 0,
    invalidStorageCall = false;
  function inspectStorage(node) {
    if (ts.isIdentifier(node) && node.text === "localStorage") {
      storageCalls++;
      const property = node.parent,
        call = property?.parent;
      if (
        !ts.isPropertyAccessExpression(property) ||
        !["getItem", "setItem"].includes(property.name.text) ||
        !ts.isCallExpression(call) ||
        !ts.isStringLiteralLike(call.arguments[0]) ||
        ![
          "opencut.library",
          "opencut.timeline",
          "opencut.layout",
          "opencut.welcome",
          "opencut.preview",
          // Finite transaction-scoped selection/scene/playhead hints only.
          // Runtime projection remains in the existing preference module.
          "opencut.history-views.v1",
          // Bounded host/project/scene view identities, no content or image bytes.
          "opencut.cover-scenes.v1",
        ].includes(call.arguments[0].text)
      )
        invalidStorageCall = true;
    }
    ts.forEachChild(node, inspectStorage);
  }
  if (viewPreferences) inspectStorage(source);
  const allowedViews =
    viewPreferences && storageCalls > 0 && !invalidStorageCall;
  // F57 intentionally keeps only the user's message/draft and 20 local mock
  // receipts. This adapter is not project, media or credential persistence.
  let feedbackStorage =
    file === "editor/feedback/mock-receiver.ts" &&
    !/\b(?:indexedDB|EditorCore|sessionStorage)\b/.test(code);
  function inspectFeedbackStorage(node) {
    if (ts.isIdentifier(node) && node.text === "localStorage") {
      const property = node.parent,
        call = property?.parent;
      if (
        !ts.isPropertyAccessExpression(property) ||
        !["getItem", "setItem"].includes(property.name.text) ||
        !ts.isCallExpression(call) ||
        !ts.isStringLiteralLike(call.arguments[0]) ||
        ![
          "opencut.feedback.draft.v1",
          "opencut.feedback.mock-history.v1",
        ].includes(call.arguments[0].text)
      )
        feedbackStorage = false;
    }
    ts.forEachChild(node, inspectFeedbackStorage);
  }
  if (feedbackStorage) inspectFeedbackStorage(source);
  // A recovery locator is not an edit payload. The sole reviewed adapter
  // projects bounded service/principal/credential/project/recovery identities.
  // All content and exact transactions remain in the authenticated host.
  const recoveryStorageKey = file === "editor/workspace/v4/timeline-edit/recovery.ts"
    ? "opencut.edit-recovery.v1"
    : file === "editor/workspace/v4/edit-recovery-pointers.ts" ? "opencut.edit-recovery.v2" : null;
  let recoveryStorage = recoveryStorageKey !== null &&
    !/\b(?:localStorage|indexedDB|EditorCore)\b/.test(code);
  function inspectRecoveryStorage(node) {
    if (ts.isIdentifier(node) && node.text === "sessionStorage") {
      if (ts.isTypeOfExpression(node.parent)) return;
      const property = node.parent, call = property?.parent;
      if (!ts.isPropertyAccessExpression(property) ||
          !["getItem", "setItem"].includes(property.name.text) ||
          !ts.isCallExpression(call) || !ts.isStringLiteralLike(call.arguments[0]) ||
          call.arguments[0].text !== recoveryStorageKey) recoveryStorage = false;
    }
    ts.forEachChild(node, inspectRecoveryStorage);
  }
  if (recoveryStorage) inspectRecoveryStorage(source);
  // One historical input adapter may read existing Classic stores. It is not
  // a modern project authority: creation/upgrade aborts, writes are forbidden,
  // and every database transaction must be explicitly readonly.
  let legacyReadOnly = file === "editor/workspace/classic/source.ts";
  function inspectLegacy(node) {
    // Computed calls can hide storage mutations from the named-method rules.
    if (
      ts.isCallExpression(node) &&
      ts.isElementAccessExpression(node.expression)
    )
      legacyReadOnly = false;
    if (ts.isPropertyAccessExpression(node)) {
      const name = node.name.text;
      // IDBOpenDBRequest.transaction is a value. The only allowed use is
      // aborting the implicit creation transaction when an old DB is absent.
      const abortAbsentCreation =
        name === "transaction" &&
        ts.isIdentifier(node.expression) &&
        node.expression.text === "open" &&
        ts.isPropertyAccessExpression(node.parent) &&
        node.parent.name.text === "abort" &&
        ts.isCallExpression(node.parent.parent);
      if (
        [
          "createObjectStore",
          "deleteObjectStore",
          "deleteDatabase",
          "put",
          "add",
          "clear",
          "delete",
          "createWritable",
          "removeEntry",
        ].includes(name)
      )
        legacyReadOnly = false;
      if (
        ["transaction", "getDirectoryHandle", "getFileHandle"].includes(name) &&
        !ts.isCallExpression(node.parent) &&
        !abortAbsentCreation
      )
        legacyReadOnly = false;
    }
    if (ts.isIdentifier(node) && node.text === "indexedDB") {
      const property = node.parent,
        call = property.parent;
      if (
        !ts.isTypeOfExpression(property) &&
        (!ts.isPropertyAccessExpression(property) ||
          property.name.text !== "open" ||
          !ts.isCallExpression(call) ||
          call.arguments.length !== 1)
      )
        legacyReadOnly = false;
    }
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression)
    ) {
      const name = node.expression.name.text;
      if (
        [
          "createObjectStore",
          "deleteObjectStore",
          "deleteDatabase",
          "put",
          "add",
          "clear",
          "delete",
          "createWritable",
          "removeEntry",
        ].includes(name)
      )
        legacyReadOnly = false;
      if (
        name === "transaction" &&
        (!node.arguments[1] ||
          !ts.isStringLiteralLike(node.arguments[1]) ||
          node.arguments[1].text !== "readonly")
      )
        legacyReadOnly = false;
      if (
        ["getDirectoryHandle", "getFileHandle"].includes(name) &&
        node.arguments.length !== 1
      )
        legacyReadOnly = false;
    }
    ts.forEachChild(node, inspectLegacy);
  }
  if (legacyReadOnly) {
    inspectLegacy(source);
    if (/\b(?:localStorage|sessionStorage|EditorCore)\b/.test(code)) legacyReadOnly = false;
  }
  if (
    /\b(?:indexedDB|EditorCore|localStorage|sessionStorage)\b/.test(code) &&
    !(file === "editor/shared/locale.ts" && !/\bsessionStorage\b/.test(code)) &&
    !allowedViews &&
    !feedbackStorage &&
    !recoveryStorage &&
    !legacyReadOnly
  )
    errors.push(`${file}: browser-owned project or credential persistence`);
  return errors;
}
export function inspect() {
  const errors = [];
  let files = 0;
  function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (
        /\.tsx?$/.test(file) &&
        !testSource(file)
      ) {
        files++;
        errors.push(
          ...violations(
            path.relative(root, file).replaceAll("\\", "/"),
            fs.readFileSync(file, "utf8"),
          ),
        );
      }
    }
  }
  walk(root);
  return { status: errors.length ? "failed" : "passed", files, errors };
}
if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  const result = inspect();
  console.log(JSON.stringify(result, null, 2));
  if (result.errors.length) process.exitCode = 1;
}
