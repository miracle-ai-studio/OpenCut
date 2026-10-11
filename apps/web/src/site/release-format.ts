export type Change = { type: string; text: string };
const sections: Record<
  string,
  { title: string; zh: string; order: number; collapsible?: boolean }
> = {
  new: { title: "Features", zh: "新增功能", order: 0 },
  improved: { title: "Improvements", zh: "改进", order: 1 },
  fixed: { title: "Fixes", zh: "修复", order: 2 },
  breaking: { title: "Breaking Changes", zh: "不兼容变更", order: 3 },
  technical: {
    title: "Technical details",
    zh: "技术说明",
    order: 4,
    collapsible: true,
  },
};
export function releaseSections(changes: readonly Change[]) {
  const groups = new Map<string, Change[]>();
  for (const change of changes) {
    const list = groups.get(change.type) ?? [];
    list.push(change);
    groups.set(change.type, list);
  }
  return [...groups]
    .map(([type, changes]) => ({
      type,
      changes,
      ...(sections[type] ?? {
        title: type.charAt(0).toUpperCase() + type.slice(1),
        zh: type,
        order: Number.MAX_SAFE_INTEGER,
      }),
    }))
    .sort((a, b) => a.order - b.order);
}
export function releaseMarkdown(release: {
  description?: string;
  changes: readonly Change[];
}) {
  const lines = release.description ? [release.description, ""] : [];
  for (const section of releaseSections(release.changes)) {
    const bullets = section.changes
      .map((change) => `- ${change.text}`)
      .join("\n");
    lines.push(
      section.collapsible
        ? `<details>\n<summary>${section.title}</summary>\n\n${bullets}\n\n</details>`
        : `## ${section.title}\n${bullets}`,
      "",
    );
  }
  return lines.join("\n").trimEnd();
}
