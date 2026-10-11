import { useState } from "react";
import { ArrowUpRight, Download } from "lucide-react";
import { brandAssets, downloadBrand } from "./brand-assets";
import { privacy, terms } from "./legal";
import { ReleasePages } from "./release-pages";
import type { Translate } from "./public-pages";
import type { SiteRoute } from "./routes";
import fal from "./assets/fal.svg";
import vercel from "./assets/vercel.svg";
export function StaticPages({ route, t }: { route: SiteRoute; t: Translate }) {
  const [busy, setBusy] = useState(false),
    [notice, setNotice] = useState("");
  const external = (href: string, label: string) => (
    <a href={href} target="_blank" rel="noopener noreferrer">
      {label}
      <ArrowUpRight size={14} />
    </a>
  );
  async function download() {
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
  return (
    <>
      {route.page === "brand" && (
        <>
          <p className="site-lead">
            {t(
              "Official OpenCut symbols, logos and wordmarks. Ready for your next project.",
              "OpenCut 原版图标、标志与文字素材，随时用于你的项目。",
            )}
          </p>
          <p>
            <a href="#guidelines">
              {t("Read the brand guidelines", "阅读品牌使用指南")} ↓
            </a>
          </p>
          <button className="site-button" disabled={busy} onClick={download}>
            <Download size={17} />
            {busy
              ? t("Preparing download…", "正在准备下载…")
              : t("Download all assets", "下载全部素材")}
          </button>
          <div className="site-brand-grid">
            {brandAssets.map((asset) => (
              <section key={asset.name}>
                <h2>{t(asset.name, asset.zh)}</h2>
                <div className="site-variants">
                  {asset.variants.map((variant) => (
                    <div
                      key={variant.file}
                      className={
                        variant.light ? "site-asset dark" : "site-asset light"
                      }
                    >
                      <img
                        src={variant.url}
                        alt={`${t(asset.name, asset.zh)} ${variant.light ? t("light", "浅色") : t("dark", "深色")}`}
                      />
                      <div>
                        <button
                          onClick={async () => {
                            try {
                              const response = await fetch(variant.url);
                              if (!response.ok) throw new Error();
                              await navigator.clipboard.writeText(
                                await response.text(),
                              );
                              setNotice(t("SVG copied", "已复制 SVG"));
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
                        <a href={variant.url} download={variant.file}>
                          {t("Download", "下载")} ↓
                        </a>
                      </div>
                    </div>
                  ))}
                </div>
              </section>
            ))}
          </div>
          <section id="guidelines" className="site-section site-prose">
            <h2>{t("Usage", "使用方式")}</h2>
            <p>
              {t(
                "OpenCut is open source: the code is free to use under its license. That license does not cover the name or logo. You can say you use OpenCut, integrate with it or build on top of it. You cannot name your product OpenCut, imply endorsement or use the marks commercially without asking first.",
                "OpenCut 是开源软件，代码可按其许可证使用。该许可不涵盖名称或标志。你可以说明使用、集成或基于 OpenCut 构建，但不能将产品命名为 OpenCut、暗示官方认可，或未经许可将标志用于商业用途。",
              )}
            </p>
            <p>
              {t("For questions, contact", "如有疑问，请联系")}{" "}
              <a href="mailto:brand@opencut.app">brand@opencut.app</a>.
            </p>
            <h2>{t("What's not allowed", "不允许的用途")}</h2>
            <ul>
              {[
                t(
                  "Using OpenCut in the name of your product, service or domain.",
                  "在产品、服务或域名中使用 OpenCut 名称。",
                ),
                t(
                  "Implying OpenCut made, sponsors or endorses your work.",
                  "暗示 OpenCut 制作、赞助或认可你的作品。",
                ),
                t(
                  "Using the name or logo on merchandise or commercial marketing without permission.",
                  "未经许可在商品或商业营销中使用名称或标志。",
                ),
                t("Modifying the marks.", "修改品牌标志。"),
              ].map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
          </section>
        </>
      )}
      {route.page === "roadmap" && (
        <>
          <p className="site-lead">
            {t(
              "A transparent view of where the original OpenCut project is heading.",
              "了解原版 OpenCut 项目的发展方向。",
            )}
          </p>
          <p className="site-muted">
            {t(
              "Original roadmap · Last updated February 25, 2026. This snapshot does not describe the completion status of this local migration.",
              "原版路线图 · 更新于 2026 年 2 月 25 日。此历史快照不代表本地迁移的完成状态。",
            )}
          </p>
          <ol className="site-roadmap">
            {[
              [
                t("Start", "起点"),
                t("Completed", "已完成"),
                t(
                  "Set the vision, open the repository and share the first announcement.",
                  "确定愿景、公开仓库，并发布首次公告。",
                ),
              ],
              [
                t("Core UI", "核心界面"),
                t("Completed", "已完成"),
                t(
                  "Build the foundations of the editor and its interface.",
                  "构建编辑器与界面的基础框架。",
                ),
              ],
              [
                t("Essential functionality", "核心功能"),
                t("In progress", "进行中"),
                t(
                  "Bring together the tools needed for everyday editing.",
                  "整合日常剪辑所需的工具。",
                ),
              ],
              [
                t("Native app", "原生应用"),
                t("Not started", "尚未开始"),
                t(
                  "Bring OpenCut to desktop and mobile.",
                  "将 OpenCut 带到桌面端和移动端。",
                ),
              ],
            ].map(([name, status, description], index) => (
              <li
                key={name}
                className={index < 2 ? "complete" : index === 2 ? "active" : ""}
              >
                <span className="site-step">{index < 2 ? "✓" : index + 1}</span>
                <div>
                  <h2>
                    {name}
                    <span>{status}</span>
                  </h2>
                  <p>{description}</p>
                  {index === 0 &&
                    external(
                      "https://x.com/mazeincoding/status/1936706642512388188",
                      t("First announcement", "首次公告"),
                    )}
                </div>
              </li>
            ))}
          </ol>
        </>
      )}
      {route.page === "sponsors" && (
        <>
          <p className="site-lead">
            {t(
              "Thank you to the partners supporting the original OpenCut community.",
              "感谢支持原版 OpenCut 社区的伙伴。",
            )}
          </p>
          <div className="site-sponsors">
            {[
              [
                "https://fal.ai?utm_source=opencut",
                fal,
                "Fal.ai",
                t(
                  "Generative image, video and audio infrastructure.",
                  "生成式图像、视频与音频基础设施。",
                ),
              ],
              [
                "https://vercel.com?utm_source=opencut",
                vercel,
                "Vercel",
                t(
                  "Hosting and delivery for the original community website.",
                  "为原版社区网站提供托管与内容分发。",
                ),
              ],
            ].map(([url, image, name, description]) => (
              <a
                key={name}
                href={url}
                target="_blank"
                rel="noopener noreferrer"
              >
                <div className="site-sponsor-logo">
                  <img src={image} alt={name} />
                </div>
                <h2>
                  {name}
                  <ArrowUpRight size={18} />
                </h2>
                <p>{description}</p>
              </a>
            ))}
          </div>
          <section className="site-section">
            <h2>{t("Support open source", "支持开源")}</h2>
            <p>
              {t(
                "Help the community build a video editor everyone can use.",
                "帮助社区构建人人可用的视频编辑器。",
              )}
            </p>
            {external(
              "https://github.com/OpenCut-app/OpenCut",
              t("Meet the project", "了解项目"),
            )}
          </section>
        </>
      )}
      {(route.page === "privacy" || route.page === "terms") && (
        <article className="site-prose">
          <p className="site-muted">
            {t(
              "Updated October 7, 2026 · Adapted from the original OpenCut policy for local processing and storage.",
              "更新于 2026 年 10 月 7 日 · 基于原版 OpenCut 政策，按本机处理与存储方式调整。",
            )}
          </p>
          {(route.page === "privacy" ? privacy : terms).map((section, index) =>
            index === 0 ? (
              <details className="site-summary" key={index} open>
                <summary>{t(...section.title)}</summary>
                <ul>
                  {section.items?.map((item, i) => (
                    <li key={i}>{t(...item)}</li>
                  ))}
                </ul>
              </details>
            ) : (
              <section key={index}>
                <h2>{t(...section.title)}</h2>
                {section.paragraphs?.map((paragraph, i) => (
                  <p key={i}>{t(...paragraph)}</p>
                ))}
                {section.items && (
                  <ul>
                    {section.items.map((item, i) => (
                      <li key={i}>{t(...item)}</li>
                    ))}
                  </ul>
                )}
              </section>
            ),
          )}
          <p>
            {external("https://github.com/OpenCut-app/OpenCut", "GitHub")} ·{" "}
            <a href="mailto:oss@opencut.app">oss@opencut.app</a> ·{" "}
            {external("https://x.com/opencutapp", "X")}
          </p>
        </article>
      )}
      {route.page === "changelog" && <ReleasePages slug={route.slug} t={t} />}
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
    </>
  );
}
