import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
const root = path.resolve(import.meta.dirname, "../src");
const features = new Set([
  "assets",
  "effects",
  "connection",
  "history",
  "inspector",
  "preview",
  "projects",
  "tasks",
  "timeline",
]);
export function violations(file, code) {
  const errors = [],
    source = ts.createSourceFile(file, code, ts.ScriptTarget.Latest, true);
  const owner = file.split("/")[1];
  const check = (name) => {
    if (typeof name !== "string") {
      errors.push(`${file}: dynamic module path`);
      return;
    }
    if (!name.startsWith(".") && !/^(?:@|#)\//.test(name)) return;
    const target = path.posix.normalize(
      /^(?:@|#)\//.test(name)
        ? name.slice(2)
        : path.posix.join(path.posix.dirname(file), name),
    );
    const dependency = target.split("/")[1];
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
    !/\b(?:indexedDB|EditorCore)\b/.test(code);
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
        ].includes(call.arguments[0].text)
      )
        invalidStorageCall = true;
    }
    ts.forEachChild(node, inspectStorage);
  }
  if (viewPreferences) inspectStorage(source);
  const allowedViews =
    viewPreferences && storageCalls > 0 && !invalidStorageCall;
  if (
    /\b(?:indexedDB|EditorCore|localStorage)\b/.test(code) &&
    file !== "editor/shared/locale.ts" &&
    !allowedViews
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
        !file.endsWith(".test.ts") &&
        !file.endsWith(".test.tsx")
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
