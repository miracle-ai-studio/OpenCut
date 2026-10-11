import { useEffect, useState } from "react";
import { ClipboardCopy } from "lucide-react";
import { releases } from "./releases";
import { releaseMarkdown, releaseSections } from "./release-format";
import { formatDate, type Translate } from "./public-pages";
function Inline({ text }: { text: string }) {
  return (
    <>
      {text
        .split(/(`[^`]+`)/)
        .map((part, index) =>
          part.startsWith("`") && part.endsWith("`") ? (
            <code key={index}>{part.slice(1, -1)}</code>
          ) : (
            part
          ),
        )}
    </>
  );
}
function Changes({
  release,
  t,
}: {
  release: (typeof releases)[number];
  t: Translate;
}) {
  return (
    <div className="site-release-sections">
      {releaseSections(release.changes).map((section) => {
        const list = (
          <ul className="site-changes">
            {section.changes.map((change, index) => (
              <li key={index}>
                <Inline text={change.text} />
              </li>
            ))}
          </ul>
        );
        return section.collapsible ? (
          <details key={section.type}>
            <summary>{t(section.title, section.zh)}</summary>
            {list}
          </details>
        ) : (
          <section key={section.type}>
            <h3>{t(section.title, section.zh)}</h3>
            {list}
          </section>
        );
      })}
    </div>
  );
}
function CopyRelease({
  release,
  t,
}: {
  release: (typeof releases)[number];
  t: Translate;
}) {
  const [notice, setNotice] = useState("");
  return (
    <>
      <button
        className="site-copy-release"
        title={t("Copy as markdown", "复制为 Markdown")}
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(releaseMarkdown(release));
            setNotice(t("Copied!", "已复制！"));
          } catch {
            setNotice(
              t("Copy failed. Please try again.", "复制失败，请重试。"),
            );
          }
        }}
      >
        <ClipboardCopy size={14} />
        {t("Copy markdown", "复制 Markdown")}
      </button>
      {notice && (
        <span role="status" className="site-muted">
          {notice}
        </span>
      )}
    </>
  );
}
export function ReleasePages({ slug, t }: { slug?: string; t: Translate }) {
  const index = releases.findIndex((release) => release.version === slug),
    release = index >= 0 ? releases[index] : undefined;
  useEffect(() => {
    if (release)
      document.title = `${release.title} (${release.version}) · OpenCut`;
  }, [release]);
  return (
    <>
      <p className="site-lead">
        {t(
          "What's new in the original OpenCut project.",
          "原版 OpenCut 的更新记录。",
        )}
      </p>
      <p className="site-muted">
        {t(
          "Published upstream release notes, in their original language. These historical notes do not certify feature availability in this local migration.",
          "以下是原版已发布的更新记录，保留原文语言。历史记录不代表本地迁移已支持其中所有功能。",
        )}
      </p>
      {slug ? (
        release && (
          <>
            <a href="/changelog">← {t("All releases", "全部更新")}</a>
            <article className="site-release-detail">
              <div className="site-release-meta">
                <span className="site-release-number">{release.version}</span>
                <time dateTime={release.date}>
                  {formatDate(release.date, t)}
                </time>
                <CopyRelease release={release} t={t} />
              </div>
              <h1>{release.title}</h1>
              <p>{release.description}</p>
              <Changes release={release} t={t} />
            </article>
            <nav
              className="site-release-nav"
              aria-label={t("Release navigation", "更新导航")}
            >
              {releases[index + 1] ? (
                <a href={`/changelog/${releases[index + 1].version}`}>
                  ← {t("Older", "较早更新")}
                  <span>{releases[index + 1].title}</span>
                </a>
              ) : (
                <span />
              )}
              {releases[index - 1] && (
                <a href={`/changelog/${releases[index - 1].version}`}>
                  {t("Newer", "较新更新")} →
                  <span>{releases[index - 1].title}</span>
                </a>
              )}
            </nav>
          </>
        )
      ) : (
        <div className="site-releases">
          {releases.map((item) => (
            <article key={item.version}>
              <aside>
                <a
                  href={`/changelog/${item.version}`}
                  className="site-release-number"
                >
                  {item.version}
                </a>
                <time dateTime={item.date}>{formatDate(item.date, t)}</time>
              </aside>
              <div>
                <h2>
                  <a href={`/changelog/${item.version}`}>{item.title}</a>
                </h2>
                <p>{item.description}</p>
                <Changes release={item} t={t} />
                <a href={`/changelog/${item.version}`}>
                  {t("Read release notes", "查看更新详情")} →
                </a>
              </div>
            </article>
          ))}
        </div>
      )}
    </>
  );
}
