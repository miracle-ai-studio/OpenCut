import { useEffect, useState } from "react";
import { ArrowUpRight, Code2, Languages, Menu, Moon, Sun } from "lucide-react";
import { preferences, useLocale } from "../editor/shared/locale";
import { useEntrance } from "../editor/shared/motion";
import { siteRoute } from "./routes";
import { downloadBrand } from "./brand-assets";
import { Blog, Contributors } from "./public-pages";
import { StaticPages } from "./static-pages";
import { Handlebars } from "./handlebars";
import symbol from "./assets/symbol.svg";
import symbolLight from "./assets/symbol-light.svg";
import text from "./assets/text.svg";
import textLight from "./assets/text-light.svg";
import landing from "./assets/landing-page-dark.png";
import headerLogo from "./assets/header-logo.svg";
import "./site.css";
const github = "https://github.com/OpenCut-app/OpenCut";
export function Site() {
  const { language, theme } = useLocale(),
    t = (en: string, zh: string) => (language === "zh" ? zh : en);
  const route = siteRoute(location.pathname),
    ref = useEntrance<HTMLElement>(`${route.page}:${route.slug}`);
  const [menu, setMenu] = useState(false),
    [brandMenu, setBrandMenu] = useState(false),
    [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  const titles = {
    home: ["OpenCut", "OpenCut"],
    brand: ["Brand assets", "品牌素材"],
    roadmap: ["Roadmap", "路线图"],
    sponsors: ["Sponsors", "赞助伙伴"],
    privacy: ["Privacy Policy", "隐私政策"],
    terms: ["Terms of Service", "服务条款"],
    contributors: ["Contributors", "贡献者"],
    blog: ["Blog", "博客"],
    changelog: ["Changelog", "更新日志"],
    "not-found": ["Page not found", "页面不存在"],
  } as const;
  const title = t(titles[route.page][0], titles[route.page][1]);
  useEffect(() => {
    if (route.slug && (route.page === "blog" || route.page === "changelog"))
      return;
    document.title =
      route.page === "home"
        ? "OpenCut — " + t("The open source video editor", "开源视频编辑器")
        : `${title} · OpenCut`;
  }, [title, language, route.page]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setBrandMenu(false);
        setMenu(false);
      }
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, []);
  async function allAssets() {
    setBusy(true);
    setNotice("");
    try {
      await downloadBrand();
    } catch {
      setNotice(t("Download failed. Please try again.", "下载失败，请重试。"));
    } finally {
      setBusy(false);
    }
  }
  const links = [
    ["/roadmap", t("Roadmap", "路线图")],
    ["/contributors", t("Contributors", "贡献者")],
    ["/sponsors", t("Sponsors", "赞助伙伴")],
    ["/changelog", t("Changelog", "更新日志")],
    ["/blog", t("Blog", "博客")],
  ];
  const external = (href: string, label: string) => (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {label}
      <ArrowUpRight size={14} />
    </a>
  );
  const logo = (
    <>
      <img src={theme === "dark" ? symbolLight : symbol} alt="" />
      <img src={theme === "dark" ? textLight : text} alt="OpenCut" />
    </>
  );
  return (
    <div className="site-root">
      <a href="#site-main" className="site-skip">
        {t("Skip to content", "跳到正文")}
      </a>
      <header className="site-header">
        <div className="site-header-inner">
          <div className="site-brand-holder">
            <a
              href="/"
              aria-label={t("OpenCut home", "OpenCut 首页")}
              className="site-logo"
              onContextMenu={(event) => {
                event.preventDefault();
                setBrandMenu(true);
              }}
            >
              <img
                className="site-header-symbol"
                src={headerLogo}
                alt="OpenCut"
              />
            </a>
            {brandMenu && (
              <div className="site-brand-menu">
                <button
                  onClick={async () => {
                    try {
                      const response = await fetch(headerLogo);
                      if (!response.ok) throw new Error();
                      await navigator.clipboard.writeText(
                        await response.text(),
                      );
                      setNotice(t("SVG copied", "已复制 SVG"));
                      setBrandMenu(false);
                    } catch {
                      setNotice(
                        t(
                          "Could not copy. Download the SVG instead.",
                          "复制失败，请下载 SVG 文件。",
                        ),
                      );
                    }
                  }}
                >
                  {t("Copy SVG", "复制 SVG")}
                </button>
                <a href={headerLogo} download="opencut-logo.svg">
                  {t("Download SVG", "下载 SVG")}
                </a>
                <a href="/brand">{t("Brand assets", "品牌素材")}</a>
                <button disabled={busy} onClick={allAssets}>
                  {t("Download all assets", "下载全部素材")}
                </button>
                <button onClick={() => setBrandMenu(false)}>
                  {t("Close", "关闭")}
                </button>
              </div>
            )}
          </div>
          <nav
            aria-label={t("Main navigation", "主导航")}
            className={menu ? "site-nav open" : "site-nav"}
          >
            {links.map(([href, label]) => (
              <a
                key={href}
                href={href}
                aria-current={location.pathname === href ? "page" : undefined}
              >
                {label}
              </a>
            ))}
            {external(
              "https://discord.com/invite/Mu3acKZvCp",
              t("Community", "社区"),
            )}
          </nav>
          <div className="site-header-actions">
            <button
              aria-label={t("Change language", "切换语言")}
              onClick={() =>
                preferences({ language: language === "en" ? "zh" : "en" })
              }
            >
              <Languages size={18} />
              <span>{language === "en" ? "中文" : "EN"}</span>
            </button>
            <button
              aria-label={t("Toggle theme", "切换主题")}
              onClick={() =>
                preferences({ theme: theme === "dark" ? "light" : "dark" })
              }
            >
              {theme === "dark" ? <Sun size={18} /> : <Moon size={18} />}
            </button>
            <a
              href={github}
              target="_blank"
              rel="noopener noreferrer"
              aria-label="GitHub"
            >
              <Code2 size={19} />
            </a>
            <a className="site-button compact" href="/projects">
              {t("Open editor", "打开编辑器")}
            </a>
            <button
              className="site-menu-toggle"
              aria-label={t("Toggle menu", "切换菜单")}
              aria-expanded={menu}
              onClick={() => setMenu((value) => !value)}
            >
              <Menu size={20} />
            </button>
          </div>
        </div>
      </header>
      <main
        id="site-main"
        ref={ref}
        className={route.page === "home" ? "site-home" : "site-main"}
      >
        {route.page === "home" ? (
          <section className="site-hero">
            <img className="site-landing" src={landing} alt="" />
            <div className="site-hero-content">
              <h1>
                {t("The open source", "开源的")}
                <br />
                <Handlebars label={t("Trim headline", "裁剪标题")}>
                  {t("Video editor", "视频编辑器")}
                </Handlebars>
              </h1>
              <p>
                {t(
                  "A simple but powerful video editor that gets the job done.",
                  "简单而强大的视频编辑器，让创作顺利完成。",
                )}
                <br />
                {t(
                  "Your stories. Your computer. Your control.",
                  "你的故事，你的电脑，由你掌控。",
                )}
              </p>
              <a href="/projects" className="site-button hero-button">
                {t("Start editing", "开始剪辑")}
                <ArrowUpRight size={18} />
              </a>
              <small>{t("Free and open source", "免费 · 开源")}</small>
            </div>
          </section>
        ) : (
          <>
            {!(
              (route.page === "blog" || route.page === "changelog") &&
              route.slug
            ) && (
              <header className="site-page-header">
                <p className="site-eyebrow">OPENCUT</p>
                <h1>{title}</h1>
              </header>
            )}
            <StaticPages route={route} t={t} />
            {route.page === "contributors" && <Contributors t={t} />}{" "}
            {route.page === "blog" && <Blog t={t} slug={route.slug} />}{" "}
            {route.page === "not-found" && (
              <>
                <p className="site-lead">
                  {t("This page does not exist.", "这个页面不存在。")}
                </p>
                <a className="site-button" href="/">
                  {t("Back home", "返回首页")}
                </a>
              </>
            )}
          </>
        )}
      </main>
      {notice && (
        <div className="site-notice" role="status">
          {notice}
          <button
            aria-label={t("Dismiss notice", "关闭提示")}
            onClick={() => setNotice("")}
          >
            ×
          </button>
        </div>
      )}
      <footer className="site-footer">
        <div className="site-footer-inner">
          <div className="site-footer-brand">
            <a href="/" className="site-logo">
              {logo}
            </a>
            <p>
              {t(
                "A simple, privacy-first video editor.",
                "简单易用，隐私优先的视频编辑器。",
              )}
            </p>
            <div className="site-socials">
              {external(github, "GitHub")}
              {external("https://x.com/opencutapp", "X")}
              {external("https://discord.com/invite/Mu3acKZvCp", "Discord")}
            </div>
          </div>
          <div>
            <h2>{t("Resources", "资源")}</h2>
            {[
              ...links,
              ["/privacy", t("Privacy", "隐私")],
              ["/terms", t("Terms", "条款")],
            ].map(([href, label]) => (
              <a key={href} href={href}>
                {label}
              </a>
            ))}
          </div>
          <div>
            <h2>{t("Community", "社区")}</h2>
            {[
              ["/contributors", t("Contributors", "贡献者")],
              ["/sponsors", t("Sponsors", "赞助伙伴")],
              ["/brand", t("Brand", "品牌")],
            ].map(([href, label]) => (
              <a key={href} href={href}>
                {label}
              </a>
            ))}
            {external(`${github}#readme`, t("About", "关于"))}
          </div>
        </div>
        <div className="site-footer-bottom">
          <span>© {new Date().getFullYear()} OpenCut</span>
          <span>
            {t("Open source. Made together.", "开源，由大家共同构建。")}
          </span>
        </div>
      </footer>
    </div>
  );
}
