import type { ProjectRef } from "../model/types";
import { compareTime, zero } from "../model/time";
export type SortKey = "name" | "created" | "modified" | "duration";
export function selectRange(
  ids: string[],
  chosen: string[],
  anchor: string | null,
  target: string,
) {
  const a = anchor ? ids.indexOf(anchor) : -1,
    b = ids.indexOf(target);
  if (a < 0 || b < 0) return [target];
  return [
    ...new Set([...chosen, ...ids.slice(Math.min(a, b), Math.max(a, b) + 1)]),
  ];
}
export function sortProjects(
  projects: ProjectRef[],
  key: SortKey,
  ascending: boolean,
) {
  return [...projects].sort((a, b) => {
    const value =
      key === "name"
        ? a.name.localeCompare(b.name)
        : key === "duration"
          ? compareTime(
              a.library?.duration ?? zero,
              b.library?.duration ?? zero,
            )
          : (
              a.library?.[key === "created" ? "created_at" : "modified_at"] ??
              ""
            ).localeCompare(
              b.library?.[key === "created" ? "created_at" : "modified_at"] ??
                "",
            );
    return (ascending ? 1 : -1) * value || a.id.localeCompare(b.id);
  });
}
export function copyNames(projects: ProjectRef[], selected: ProjectRef[]) {
  const parse = (name: string) => {
    const match = /^\((\d+)\)\s+(.+)$/.exec(name);
    return match && Number.isSafeInteger(Number(match[1]))
      ? { base: match[2], number: Number(match[1]) }
      : { base: name, number: 0 };
  };
  const next = new Map<string, number>();
  for (const project of projects) {
    const { base, number } = parse(project.name);
    next.set(base, Math.max(next.get(base) ?? 1, number + 1));
  }
  return selected.map((project) => {
    const { base } = parse(project.name);
    const number = next.get(base) ?? 1;
    next.set(base, number + 1);
    return `(${number}) ${base}`;
  });
}
