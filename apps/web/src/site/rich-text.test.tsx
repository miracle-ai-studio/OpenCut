// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, expect, it, vi } from "vitest";
import { richText } from "./rich-text";

afterEach(() => {
  cleanup();
  history.replaceState(null, "", "/");
  vi.restoreAllMocks();
});
it("preserves unique upstream IDs, generates Classic Unicode slugs and resolves collisions deterministically", () => {
  const html =
    '<h2>Hello, World!</h2><h2 id="hello-world">Pinned title</h2><h2>Hello, World!</h2><h2>HELLO-WORLD-1</h2><h2>Привет non-latin 你好</h2><h2>emoji 😄</h2><h2>Café Déjà_vu</h2><h2>  padded  title </h2>';
  const { container } = render(<>{richText(html)}</>);
  expect(
    [...container.querySelectorAll("h2")].map((heading) => heading.id),
  ).toEqual([
    "hello-world-1",
    "hello-world",
    "hello-world-2",
    "hello-world-1-1",
    "привет-non-latin-你好",
    "emoji-",
    "café-déjà_vu",
    "--padded--title-",
  ]);
  expect(renderToStaticMarkup(richText(html))).toBe(
    renderToStaticMarkup(richText(html)),
  );
  const generated = screen.getByRole("link", { name: "Привет non-latin 你好" });
  expect(generated.getAttribute("href")).toBe(
    `#${encodeURIComponent("привет-non-latin-你好")}`,
  );
  expect(generated.tabIndex).toBe(0);
});
it("keeps an existing automatic heading link accessible instead of duplicating it or copying hidden/tabindex attributes", () => {
  render(
    <>
      {richText(
        '<h2 id="source-heading">Heading<a href="#source-heading" aria-hidden="true" tabindex="-1"><span class="icon"></span></a></h2>',
      )}
    </>,
  );
  const links = screen.getAllByRole("link");
  expect(links).toHaveLength(1);
  expect(links[0].getAttribute("href")).toBe("#source-heading");
  expect(links[0].tabIndex).toBe(0);
  expect(links[0].getAttribute("aria-hidden")).toBeNull();
  expect(links[0].getAttribute("aria-label")).toBe("Heading");
});
it("admits single-decoded local fragments only for real sanitized heading targets, with external URL rules unchanged", () => {
  const { container } = render(
    <>
      {richText(
        '<a href="#%E4%BD%A0%E5%A5%BD">Jump</a><h2 id="你好">Title</h2><p id="not-a-heading">Text</p><a href="#not-a-heading">discarded target</a><a href="/blog/elsewhere#你好">relative</a><a href="javascript:evil">script</a><a href="#%252Fprivate">double encoded</a><a href="#%2Fprivate">encoded slash</a><a href="#%ZZ">malformed</a><a href="https://example.com/post#safe" onclick="evil">External</a>',
      )}
    </>,
  );
  expect(screen.getByRole("link", { name: "Jump" }).getAttribute("href")).toBe(
    `#${encodeURIComponent("你好")}`,
  );
  expect(
    screen.getByRole("link", { name: "External" }).getAttribute("rel"),
  ).toBe("noopener noreferrer");
  expect(
    screen.getByRole("link", { name: "External" }).getAttribute("target"),
  ).toBe("_blank");
  expect(
    screen.getByRole("link", { name: "Jump" }).getAttribute("target"),
  ).toBeNull();
  expect(container.querySelectorAll("a")).toHaveLength(3); // Jump, generated heading link, External.
  expect(container.querySelector("p")!.getAttribute("id")).toBeNull();
  expect(container.innerHTML).not.toContain("onclick");
});
it("bounds and uniquifies empty/invalid/duplicate IDs without retaining discarded markup or shell collisions", () => {
  const { container } = render(
    <>
      {richText(
        `<h2 id="root">Root heading</h2><h2 id="dup">One</h2><h2 id="dup">Two</h2><h2 id="bad id">Three</h2><h2>😄</h2><h2>😄</h2><h2>${"A".repeat(400)}</h2><script><h2 id="hidden">bad</h2></script>`,
      )}
    </>,
  );
  const ids = [...container.querySelectorAll("h2")].map(
    (heading) => heading.id,
  );
  expect(ids.slice(0, 6)).toEqual([
    "root-heading",
    "dup",
    "two",
    "three",
    "section",
    "section-1",
  ]);
  expect(ids.every((id) => id.length > 0 && id.length <= 256)).toBe(true);
  expect(new Set(ids).size).toBe(ids.length);
  expect(container.querySelector("script")).toBeNull();
  expect(container.querySelector("#hidden")).toBeNull();
});
it("restores a still-current initial fragment after async article mounting without stealing an existing input focus", async () => {
  history.replaceState(null, "", "/#late-heading");
  render(<>{richText('<h2 id="late-heading">Late heading</h2>')}</>);
  await waitFor(() =>
    expect(document.activeElement).toBe(
      screen.getByRole("heading", { level: 2 }),
    ),
  );
  cleanup();
  render(<input aria-label="Existing control" />);
  screen.getByLabelText("Existing control").focus();
  const input = document.activeElement;
  render(<>{richText('<h2 id="late-heading">Late heading</h2>')}</>);
  await Promise.resolve();
  expect(document.activeElement).toBe(input);
});
