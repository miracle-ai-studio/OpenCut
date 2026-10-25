export function isEditorPath(path: string) {
  return (
    path === "/" ||
    path === "/projects" ||
    path === "/editor" ||
    path.startsWith("/editor/")
  );
}

export function editorProjectId(path: string): string | null {
  if (!path.startsWith("/editor/")) return null;
  const segment = path.slice("/editor/".length);
  if (!segment || segment.includes("/") || segment.includes("\\")) return null;
  try {
    const id = decodeURIComponent(segment);
    return id.trim() &&
      id !== "." &&
      id !== ".." &&
      !/[\u0000-\u001f\u007f-\u009f]/.test(id)
      ? id
      : null;
  } catch {
    return null;
  }
}
export function editorProjectPath(id: string) {
  return `/editor/${encodeURIComponent(id)}`;
}
