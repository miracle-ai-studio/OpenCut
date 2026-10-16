// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
const api = vi.hoisted(() => ({
  posts: vi.fn(),
  post: vi.fn(),
  contributors: vi.fn(),
}));
vi.mock("./public-data", () => api);
import { Blog, Contributors, blogPageFromSearch } from "./public-pages";
const t = (en: string) => en;
const article = (slug: string) => ({
  slug,
  id: slug,
  title: `Title ${slug}`,
  description: "Description",
  content: "",
  cover: null,
  date: "2026-10-16",
  authors: [],
});
const page = (number: number) => ({
  page: number,
  totalPages: 3,
  nextPage: number < 3 ? number + 1 : null,
  posts: [article(`post-${number}`)],
});
beforeEach(() => {
  history.replaceState(null, "", "/blog");
  api.posts.mockImplementation(async (number) => page(number));
  api.post.mockImplementation(async (slug) => article(slug));
  api.contributors.mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  history.replaceState(null, "", "/");
});

it("opens the requested list page directly and retains its page through detail and All posts links", async () => {
  history.replaceState(null, "", "/blog?page=2");
  const view = render(<Blog t={t} />);
  expect(
    (await screen.findByRole("link", { name: /Title post-2/ })).getAttribute(
      "href",
    ),
  ).toBe("/blog/post-2?page=2");
  expect(api.posts).toHaveBeenCalledWith(2, expect.any(AbortSignal));
  view.unmount();
  history.replaceState(null, "", "/blog/post-2?page=2");
  render(<Blog t={t} slug="post-2" />);
  expect(
    (await screen.findByRole("link", { name: /All posts/ })).getAttribute(
      "href",
    ),
  ).toBe("/blog?page=2");
});

it("updates the URL on pagination and responds to browser history without remounting", async () => {
  render(<Blog t={t} />);
  await screen.findByRole("link", { name: /Title post-1/ });
  fireEvent.click(screen.getByRole("button", { name: "Next" }));
  expect(location.search).toBe("?page=2");
  await screen.findByRole("link", { name: /Title post-2/ });
  act(() => {
    history.replaceState(null, "", "/blog?page=3");
    window.dispatchEvent(new PopStateEvent("popstate"));
  });
  await screen.findByRole("link", { name: /Title post-3/ });
  act(() => {
    history.replaceState(null, "", "/blog");
    window.dispatchEvent(new PopStateEvent("popstate"));
  });
  await screen.findByRole("link", { name: /Title post-1/ });
  expect(
    (screen.getByRole("button", { name: "Previous" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});

it("discards a slow old page response after history has selected another page", async () => {
  let finish!: (value: ReturnType<typeof page>) => void;
  api.posts.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  render(<Blog t={t} />);
  await waitFor(() => expect(api.posts).toHaveBeenCalledOnce());
  act(() => {
    history.replaceState(null, "", "/blog?page=2");
    window.dispatchEvent(new PopStateEvent("popstate"));
  });
  await screen.findByRole("link", { name: /Title post-2/ });
  await act(async () => finish(page(1)));
  expect(screen.queryByRole("link", { name: /Title post-1/ })).toBeNull();
  expect(screen.getByRole("link", { name: /Title post-2/ }).isConnected).toBe(
    true,
  );
});

it.each([
  "?page=0",
  "?page=-1",
  "?page=21",
  "?page=1&page=2",
  "?page=NaN",
  "?page=999999999999999999",
  "?page=01",
])("rejects invalid page %s without an upstream read", async (search) => {
  history.replaceState(null, "", `/blog${search}`);
  expect(blogPageFromSearch(search)).toBeNull();
  render(<Blog t={t} />);
  await screen.findByText("This blog page could not be found.");
  expect(api.posts).not.toHaveBeenCalled();
});

it("preserves the original external tool credit links without enabling analytics", async () => {
  render(<Contributors t={t} />);
  await screen.findByRole("link", { name: "Marble" });
  expect(
    screen.getByRole("link", { name: "Marble" }).getAttribute("href"),
  ).toBe("https://marblecms.com?utm_source=opencut");
  expect(
    screen.getByRole("link", { name: "Databuddy" }).getAttribute("href"),
  ).toBe("https://databuddy.cc?utm_source=opencut");
  expect(document.querySelector("script[src*=databuddy]")).toBeNull();
});
