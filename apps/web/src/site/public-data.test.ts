// @vitest-environment jsdom
import { afterEach, describe, it, expect, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  parseContributors,
  parsePost,
  parsePosts,
  safePublicUrl,
  post,
  posts,
} from "./public-data";
import { richText } from "./rich-text";
import { siteRoute, isEditorPath } from "./routes";
import { zipAssets } from "./brand-assets";
afterEach(() => vi.unstubAllGlobals());
describe("public site boundaries", () => {
  it("reads public blog metadata from the fixed local adapter without editor credentials", async () => {
    const value = { slug: "known", title: "Source title", authors: [] };
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            posts: [value],
            pagination: { currpage: 1, totalPages: 1, nextPage: null },
          }),
        ),
      )
      .mockResolvedValueOnce(new Response(JSON.stringify({ post: value })))
      .mockResolvedValueOnce(new Response(null, { status: 404 }));
    vi.stubGlobal("fetch", fetcher);
    const signal = new AbortController().signal;
    expect((await posts(1, signal)).posts[0].title).toBe("Source title");
    expect((await post("known", signal)).slug).toBe("known");
    await expect(post("missing", signal)).rejects.toThrow("not-found");
    await expect(post("../private-token", signal)).rejects.toThrow("not-found");
    expect(fetcher.mock.calls.map(([url]) => url)).toEqual([
      "/app/public/v1/blog?page=1",
      "/app/public/v1/blog/known",
      "/app/public/v1/blog/missing",
    ]);
    for (const [, options] of fetcher.mock.calls) {
      expect(options.credentials).toBe("omit");
      expect(options.headers).toEqual({ Accept: "application/json" });
      expect(options.referrerPolicy).toBe("no-referrer");
    }
  });
  it("keeps declared public routes separate from authenticated editor assembly", () => {
    expect(siteRoute("/").page).toBe("home");
    expect(siteRoute("/blog/a-slug")).toEqual({ page: "blog", slug: "a-slug" });
    expect(siteRoute("/changelog/0.3.0").slug).toBe("0.3.0");
    expect(siteRoute("/changelog/0.4.0").page).toBe("not-found");
    expect(siteRoute("/blog/a%2fb").page).toBe("not-found");
    expect(siteRoute("/blog/%xx").page).toBe("not-found");
    expect(isEditorPath("/editor/project")).toBe(true);
    expect(isEditorPath("/blog/editor")).toBe(false);
  });
  it("accepts bounded public data and drops unsafe links or identities", () => {
    expect(safePublicUrl("javascript:alert(1)")).toBeNull();
    expect(safePublicUrl("https://user:password@example.com")).toBeNull();
    expect(
      parseContributors([
        {
          id: 1,
          type: "User",
          login: "person",
          html_url: "https://github.com/person",
          avatar_url: "https://avatars.githubusercontent.com/person",
          contributions: 10,
        },
        { id: 2, type: "Bot", login: "bot" },
        {
          id: 3,
          type: "User",
          login: "bad",
          html_url: "https://example.com",
          avatar_url: "https://example.com",
          contributions: -1,
        },
      ]),
    ).toHaveLength(1);
    const p = {
      slug: "hello",
      title: "Hello",
      content: "<p>Text</p>",
      coverImage: "javascript:x",
      authors: [{ name: "Author", image: "data:text/x" }],
    };
    expect(parsePost(p)?.cover).toBeNull();
    expect(parsePost({ ...p, slug: "../bad" })).toBeNull();
    expect(
      parsePosts({
        posts: [p],
        pagination: { currpage: 1, nextPage: 2, totalPages: 2 },
      }).nextPage,
    ).toBe(2);
    expect(
      parsePosts({
        posts: [p],
        pagination: { currpage: 1, nextPage: 900, totalPages: 2 },
      }).nextPage,
    ).toBeNull();
  });
  it("renders rich text without executable markup or unsafe resource URLs", () => {
    const result = renderToStaticMarkup(
      richText(
        '<h2>Title</h2><script>alert(1)</script><iframe src="https://example.com"></iframe><p onclick="alert(2)">Safe <a href="javascript:evil">label</a><a href="https://example.com" style="x">link</a><img src="data:evil"><img src="https://example.com/a.png" onerror="evil" alt="Photo"></p>',
      ),
    );
    expect(result).toContain('<h2 id="title" tabindex="-1">Title');
    expect(result).toContain('href="#title"');
    expect(result).toContain("noopener noreferrer");
    expect(result).toContain('alt="Photo"');
    expect(result).not.toMatch(
      /script|iframe|javascript|onclick|onerror|style=/,
    );
  });
  it("creates a stored ZIP with independent CRC32 and canonical filenames", () => {
    const bytes = zipAssets([
      { name: "symbol.svg", bytes: new TextEncoder().encode("123456789") },
    ]);
    const view = new DataView(bytes.buffer);
    expect(view.getUint32(0, true)).toBe(0x04034b50);
    expect(view.getUint32(14, true)).toBe(0xcbf43926);
    expect(view.getUint32(bytes.length - 22, true)).toBe(0x06054b50);
    expect(view.getUint16(bytes.length - 14, true)).toBe(1);
    expect(() =>
      zipAssets([{ name: "../token.svg", bytes: new Uint8Array() }]),
    ).toThrow();
  });
});
