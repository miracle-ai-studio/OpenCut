import { useEffect, useMemo, useState } from "react";
import {
  contributors,
  posts,
  post,
  type Contributor,
  type Post,
  type PostPage,
} from "./public-data";
import { richText } from "./rich-text";
export type Translate = (en: string, zh: string) => string;
function usePublic<T>(load: (signal: AbortSignal) => Promise<T>, key: string) {
  const [state, setState] = useState<{
      key: string;
      data?: T;
      error?: string;
      loading: boolean;
    }>({ key, loading: true }),
    [retry, setRetry] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    setState({ key, loading: true });
    load(abort.signal).then(
      (data) => {
        if (!abort.signal.aborted) setState({ key, data, loading: false });
      },
      (error) => {
        if (!abort.signal.aborted)
          setState({
            key,
            error: error instanceof Error ? error.message : "failed",
            loading: false,
          });
      },
    );
    return () => abort.abort();
  }, [key, retry]);
  const current: typeof state =
    state.key === key ? state : { key, loading: true };
  return { ...current, retry: () => setRetry((value) => value + 1) };
}
function Failure({
  t,
  retry,
  empty = false,
}: {
  t: Translate;
  retry: () => void;
  empty?: boolean;
}) {
  return (
    <div className="site-empty" role="status">
      <p>
        {empty
          ? t("No posts published yet.", "暂无公开文章。")
          : t(
              "Public content could not be loaded. Please try again.",
              "暂时无法读取公开内容，请重试。",
            )}
      </p>
      <button className="site-button" onClick={retry}>
        {t("Try again", "重试")}
      </button>
    </div>
  );
}
export function Contributors({ t }: { t: Translate }) {
  const state = usePublic<Contributor[]>(contributors, "contributors");
  return (
    <>
      <p className="site-lead">
        {t(
          "Meet the people building OpenCut. Every contribution counts.",
          "认识共同构建 OpenCut 的人们。每一份贡献都很重要。",
        )}
      </p>
      {state.loading ? (
        <p role="status">{t("Loading contributors…", "正在读取贡献者…")}</p>
      ) : state.error ? (
        <Failure t={t} retry={state.retry} />
      ) : (
        <>
          <div className="site-stats">
            <div>
              <strong>{state.data?.length ?? 0}</strong>
              <span>{t("Contributors", "贡献者")}</span>
            </div>
            <div>
              <strong>
                {state.data?.reduce(
                  (total, person) => total + person.contributions,
                  0,
                ) ?? 0}
              </strong>
              <span>{t("Contributions", "贡献")}</span>
            </div>
          </div>
          <div className="site-contributors">
            {state.data?.map((person, index) => (
              <a
                key={person.id}
                className={
                  index < 2 ? "site-contributor featured" : "site-contributor"
                }
                href={person.url}
                target="_blank"
                rel="noopener noreferrer"
              >
                <img
                  src={person.avatar}
                  alt=""
                  loading="lazy"
                  referrerPolicy="no-referrer"
                />
                <span>
                  <strong>{person.login}</strong>
                  <small>
                    {person.contributions} {t("contributions", "次贡献")}
                  </small>
                </span>
              </a>
            ))}
          </div>
        </>
      )}
      <section className="site-section">
        <h2>{t("Built with the community", "与社区一起构建")}</h2>
        <p>
          {t(
            "Share ideas, report an issue or help improve the editor.",
            "分享想法、报告问题，或帮助改进编辑器。",
          )}
        </p>
        <a
          className="site-button"
          href="https://github.com/OpenCut-app/OpenCut"
          target="_blank"
          rel="noopener noreferrer"
        >
          {t("Contribute on GitHub", "在 GitHub 上贡献")}
        </a>
        <p className="site-muted">
          {t(
            "The original community website uses Marble for its blog and Databuddy for analytics. This local application does not include Databuddy.",
            "原版社区网站使用 Marble 管理博客，并使用 Databuddy 进行分析。本地应用未接入 Databuddy。",
          )}
        </p>
        <p className="site-muted">
          <a
            href="https://marblecms.com?utm_source=opencut"
            target="_blank"
            rel="noopener noreferrer"
          >
            Marble
          </a>
          {" · "}
          <a
            href="https://databuddy.cc?utm_source=opencut"
            target="_blank"
            rel="noopener noreferrer"
          >
            Databuddy
          </a>
        </p>
      </section>
    </>
  );
}
/** Mirrors the fixed local adapter's 20-page admission. Unknown page values do
 * not silently display page one or initiate an unbounded upstream request. */
export function blogPageFromSearch(search: string): number | null {
  const pages = new URLSearchParams(search).getAll("page");
  if (pages.length === 0) return 1;
  if (pages.length !== 1 || !/^[1-9][0-9]?$/u.test(pages[0])) return null;
  const page = Number(pages[0]);
  return page <= 20 ? page : null;
}
export const blogListHref = (page: number | null) =>
  page !== null && page > 1 ? `/blog?page=${page}` : "/blog";
export function Blog({ t, slug }: { t: Translate; slug?: string }) {
  const [page, setPage] = useState(() =>
      blogPageFromSearch(
        typeof location === "undefined" ? "" : location.search,
      ),
    ),
    state = usePublic<PostPage | Post>(
      (signal) =>
        slug
          ? post(slug, signal)
          : page === null
            ? Promise.reject(new Error("invalid-blog-page"))
            : posts(page, signal),
      `${slug ?? "list"}:${page}`,
    );
  useEffect(() => {
    const onHistory = () => setPage(blogPageFromSearch(location.search));
    window.addEventListener("popstate", onHistory);
    return () => window.removeEventListener("popstate", onHistory);
  }, []);
  const goPage = (next: number) => {
    if (!Number.isInteger(next) || next < 1 || next > 20 || next === page)
      return;
    const url = new URL(location.href);
    if (next === 1) url.searchParams.delete("page");
    else url.searchParams.set("page", String(next));
    history.pushState(null, "", `${url.pathname}${url.search}${url.hash}`);
    setPage(next);
  };
  const article =
    state.data && "content" in state.data ? state.data : undefined;
  useEffect(() => {
    if (article) document.title = `${article.title} · OpenCut`;
  }, [article?.title]);
  const content = useMemo(
    () => (article ? richText(article.content) : null),
    [article],
  );
  if (state.loading)
    return <p role="status">{t("Loading posts…", "正在读取文章…")}</p>;
  if (state.error === "invalid-blog-page")
    return (
      <p role="status">
        {t("This blog page could not be found.", "找不到这一博客页。")}
      </p>
    );
  if (state.error)
    return state.error === "not-found" ? (
      <p>{t("This article could not be found.", "找不到这篇文章。")}</p>
    ) : (
      <Failure t={t} retry={state.retry} />
    );
  if (article)
    return (
      <article className="site-prose site-article">
        <a href={blogListHref(page)}>← {t("All posts", "全部文章")}</a>
        <header>
          <p className="site-muted">
            {article.authors.map((author) => author.name).join(", ")} ·{" "}
            <time>{formatDate(article.date, t)}</time>
          </p>
          <h1>{article.title}</h1>
          <p className="site-lead">{article.description}</p>
        </header>
        {article.cover && (
          <img
            className="site-cover"
            src={article.cover}
            alt=""
            referrerPolicy="no-referrer"
          />
        )}
        <p className="site-muted">
          {t(
            "Original community article; published in its original language.",
            "原版社区文章，保留原文语言。",
          )}
        </p>
        {content}
      </article>
    );
  const list = state.data && "posts" in state.data ? state.data : undefined;
  return (
    <>
      <p className="site-lead">
        {t(
          "News, stories and updates from the OpenCut community.",
          "来自 OpenCut 社区的消息、故事和更新。",
        )}
      </p>
      <p className="site-muted">
        {t(
          "Articles are presented in their original language.",
          "文章保留发布时的原文语言。",
        )}
      </p>
      {!list?.posts.length ? (
        <Failure t={t} retry={state.retry} empty />
      ) : (
        <div className="site-posts">
          {list.posts.map((item) => (
            <a
              key={item.slug}
              className="site-post"
              href={`/blog/${encodeURIComponent(item.slug)}${page !== null && page > 1 ? `?page=${page}` : ""}`}
            >
              <div>
                <time>{formatDate(item.date, t)}</time>
                <h2>{item.title}</h2>
                <p>{item.description}</p>
                <small>
                  {item.authors.map((author) => author.name).join(", ")}
                </small>
              </div>
            </a>
          ))}
        </div>
      )}
      {list && list.totalPages > 1 && (
        <nav
          className="site-pagination"
          aria-label={t("Blog pages", "博客分页")}
        >
          <button
            disabled={(page ?? 1) <= 1}
            onClick={() => goPage((page ?? 1) - 1)}
          >
            {t("Previous", "上一页")}
          </button>
          <span>
            {page} / {list.totalPages}
          </span>
          <button
            disabled={!list.nextPage || list.nextPage > 20}
            onClick={() => list.nextPage && goPage(list.nextPage)}
          >
            {t("Next", "下一页")}
          </button>
        </nav>
      )}
    </>
  );
}
export function formatDate(value: string, t: Translate) {
  const date = new Date(value);
  return Number.isFinite(date.getTime())
    ? date.toLocaleDateString(t("en", "zh-CN"), {
        year: "numeric",
        month: "long",
        day: "numeric",
      })
    : "";
}
