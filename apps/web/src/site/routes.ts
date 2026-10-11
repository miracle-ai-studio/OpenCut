import { releases } from "./releases";
export type SiteRoute = {
  page:
    | "home"
    | "brand"
    | "roadmap"
    | "sponsors"
    | "privacy"
    | "terms"
    | "contributors"
    | "blog"
    | "changelog"
    | "not-found";
  slug?: string;
};
export function siteRoute(path: string): SiteRoute {
  let parts: string[];
  try {
    parts = path
      .replace(/\/+$/, "")
      .split("/")
      .slice(1)
      .map(decodeURIComponent);
  } catch {
    return { page: "not-found" };
  }
  if (!parts.length || parts[0] === "") return { page: "home" };
  const page = parts[0];
  if (
    parts.length === 1 &&
    [
      "brand",
      "roadmap",
      "sponsors",
      "privacy",
      "terms",
      "contributors",
      "blog",
      "changelog",
    ].includes(page)
  )
    return { page: page as SiteRoute["page"] };
  if (
    parts.length === 2 &&
    page === "changelog" &&
    releases.some((r) => r.version === parts[1])
  )
    return { page, slug: parts[1] };
  if (
    parts.length === 2 &&
    page === "blog" &&
    /^[a-zA-Z0-9_-]{1,200}$/.test(parts[1])
  )
    return { page, slug: parts[1] };
  return { page: "not-found" };
}
export { isEditorPath } from "../routes";
