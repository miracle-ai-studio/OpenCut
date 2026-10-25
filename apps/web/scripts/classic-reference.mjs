import path from "node:path";

// Independent oracles may inspect the fixed upstream checkout, which is an
// explicit external test input rather than a second product tree in this repo.
export function classicReference(relative) {
  const configured = process.env.OPENCUT_CLASSIC_SOURCE_ROOT;
  if (!configured || !path.isAbsolute(configured))
    throw new Error("Set OPENCUT_CLASSIC_SOURCE_ROOT to the absolute fixed Classic reference checkout");
  const root = path.resolve(configured);
  const file = path.resolve(root, relative);
  if (!file.startsWith(root + path.sep)) throw new Error("Reference path escapes the fixed checkout");
  return file;
}
