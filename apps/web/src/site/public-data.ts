export type Contributor = {
  id: number;
  login: string;
  avatar: string;
  url: string;
  contributions: number;
};
export type Post = {
  id: string;
  slug: string;
  title: string;
  description: string;
  content: string;
  cover: string | null;
  date: string;
  authors: { name: string; image: string | null }[];
};
export function safePublicUrl(value: unknown): string | null {
  if (typeof value !== "string" || value.length > 2048) return null;
  try {
    const url = new URL(value);
    return url.protocol === "https:" && !url.username && !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}
const object = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
const string = (value: unknown, max = 1000) =>
  typeof value === "string" ? value.slice(0, max) : "";
export function parseContributors(value: unknown): Contributor[] {
  if (!Array.isArray(value)) throw new Error("invalid public response");
  return value.slice(0, 100).flatMap((entry) => {
    const item = object(entry);
    if (
      !item ||
      item.type !== "User" ||
      !Number.isSafeInteger(item.id) ||
      !Number.isSafeInteger(item.contributions) ||
      Number(item.contributions) < 0 ||
      !/^[a-zA-Z0-9-]{1,100}$/.test(string(item.login))
    )
      return [];
    const avatar = safePublicUrl(item.avatar_url),
      url = safePublicUrl(item.html_url);
    if (!avatar || !url || new URL(url).hostname !== "github.com") return [];
    return [
      {
        id: Number(item.id),
        login: String(item.login),
        avatar,
        url,
        contributions: Number(item.contributions),
      },
    ];
  });
}
export function parsePost(value: unknown): Post | null {
  const item = object(value);
  if (
    !item ||
    !/^[a-zA-Z0-9_-]{1,200}$/.test(string(item.slug)) ||
    !string(item.title)
  )
    return null;
  return {
    id: string(item.id),
    slug: String(item.slug),
    title: string(item.title, 300),
    description: string(item.description, 3000),
    content: string(item.content, 500000),
    cover: safePublicUrl(item.coverImage),
    date: string(item.publishedAt, 100),
    authors: Array.isArray(item.authors)
      ? item.authors.slice(0, 10).flatMap((author) => {
          const a = object(author);
          return a && string(a.name)
            ? [{ name: string(a.name, 100), image: safePublicUrl(a.image) }]
            : [];
        })
      : [],
  };
}
export type PostPage = {
  posts: Post[];
  page: number;
  totalPages: number;
  nextPage: number | null;
};
export function parsePosts(value: unknown): PostPage {
  const data = object(value),
    pagination = object(data?.pagination);
  if (!data || !Array.isArray(data.posts))
    throw new Error("invalid public response");
  const page = Number(pagination?.currpage),
    total = Number(pagination?.totalPages),
    next = Number(pagination?.nextPage);
  return {
    posts: data.posts.slice(0, 50).flatMap((item) => {
      const post = parsePost(item);
      return post ? [post] : [];
    }),
    page: Number.isSafeInteger(page) && page > 0 ? page : 1,
    totalPages: Number.isSafeInteger(total) && total > 0 ? total : 1,
    nextPage:
      pagination?.nextPage != null &&
      Number.isSafeInteger(next) &&
      next > page &&
      next <= total
        ? next
        : null,
  };
}
async function readPublicJson(
  url: string,
  signal: AbortSignal,
): Promise<unknown> {
  const timeout = AbortSignal.timeout(15000);
  const response = await fetch(url, {
    signal: AbortSignal.any([signal, timeout]),
    credentials: "omit",
    referrerPolicy: "no-referrer",
    headers: { Accept: "application/json" },
  });
  if (!response.ok)
    throw new Error(
      response.status === 404 ? "not-found" : "public-request-failed",
    );
  if (!response.body) throw new Error("empty response");
  const reader = response.body.getReader();
  let size = 0;
  const parts: Uint8Array[] = [];
  try {
    for (;;) {
      const result = await reader.read();
      if (result.done) break;
      size += result.value.length;
      if (size > 4 * 1024 * 1024) throw new Error("public response too large");
      parts.push(result.value);
    }
  } catch (error) {
    await reader.cancel().catch(() => {});
    throw error;
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.length;
  }
  return JSON.parse(new TextDecoder().decode(bytes));
}
const marble = "https://api.marblecms.com/cmd4iw9mm0006l804kwqv0k46";
export async function contributors(signal: AbortSignal) {
  return parseContributors(
    await readPublicJson(
      "https://api.github.com/repos/OpenCut-app/OpenCut/contributors?per_page=100",
      signal,
    ),
  );
}
export async function posts(page: number, signal: AbortSignal) {
  return parsePosts(
    await readPublicJson(
      `${marble}/posts?page=${Number.isSafeInteger(page) && page > 0 ? page : 1}`,
      signal,
    ),
  );
}
export async function post(slug: string, signal: AbortSignal) {
  if (!/^[a-zA-Z0-9_-]{1,200}$/.test(slug)) throw new Error("not-found");
  const value = object(
    await readPublicJson(`${marble}/posts/${encodeURIComponent(slug)}`, signal),
  );
  const result = parsePost(value?.post);
  if (!result) throw new Error("not-found");
  return result;
}
