import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import {
  AudioLines,
  Captions,
  Clapperboard,
  FolderOpen,
  Image,
  Music2,
  Shapes,
  Sparkles,
  Type,
  Layers,
} from "lucide-react";
import { t, useLocale } from "../../../shared/locale";
import { useEditingWorkspace as useWorkspace } from "../../context";
import { MediaPanel } from "../media-panel";
import { ResourceCatalogPanel } from "../catalog_resources-panel";
import { ProjectFontCard } from "../text/font-card";
import { TextActions } from "../text/workspace";
import { EffectsResources } from "./effects";
import type { Sequence } from "../types";
import { TransitionResources } from "./transitions";
import { useResourceCopy } from "./resources-copy";

const tabs = [
  { id: "media", label: "tabMedia", icon: Clapperboard },
  { id: "audio", label: "tabAudio", icon: Music2 },
  { id: "text", label: "tabText", icon: Type },
  { id: "graphic", label: "tabGraphic", icon: Shapes },
  { id: "effect", label: "tabEffect", icon: Sparkles },
  { id: "transition", label: "tabTransition", icon: Layers },
  { id: "caption", label: "tabCaption", icon: Captions },
] as const;
type ResourceTab = (typeof tabs)[number]["id"];

export function WorkbenchResources({
  captions,
  sequence,
}: {
  captions: ReactNode;
  sequence: Sequence;
}) {
  useLocale();
  const c = useResourceCopy();
  const { scene } = useWorkspace();
  const [tab, setTab] = useState<ResourceTab>("media");
  const [filter, setFilter] = useState<"all" | "video" | "audio" | "image">(
    "all",
  );
  const [audioSource, setAudioSource] = useState<"local" | "catalog">(
    "catalog",
  );
  const [graphicSource, setGraphicSource] = useState<
    "shapes" | "flags" | "brands"
  >("shapes");
  const mediaVisible =
    tab === "media" || (tab === "audio" && audioSource === "local");
  const catalogVisible =
    tab === "graphic" || (tab === "audio" && audioSource === "catalog");
  const catalogKey = tab === "graphic" ? graphicSource : "sounds";
  const catalogView = useRef<HTMLDivElement>(null);
  const catalogPositions = useRef(new Map<string, number>());
  useLayoutEffect(() => {
    if (catalogVisible && catalogView.current)
      catalogView.current.scrollTop =
        catalogPositions.current.get(catalogKey) ?? 0;
  }, [catalogKey, catalogVisible]);
  return (
    <>
      <nav
        className="workbench-resource-tabs"
        role="tablist"
        aria-label={t("Editing tools")}
      >
        {tabs.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            id={`resource-${id}-tab`}
            role="tab"
            aria-controls="resource-active-view"
            aria-selected={tab === id}
            tabIndex={tab === id ? 0 : -1}
            onClick={() => setTab(id)}
            onKeyDown={(event) => {
              const index = tabs.findIndex((item) => item.id === id);
              const next =
                event.key === "ArrowRight"
                  ? (index + 1) % tabs.length
                  : event.key === "ArrowLeft"
                    ? (index + tabs.length - 1) % tabs.length
                    : event.key === "Home"
                      ? 0
                      : event.key === "End"
                        ? tabs.length - 1
                        : null;
              if (next !== null) {
                event.preventDefault();
                setTab(tabs[next].id);
                document
                  .getElementById(`resource-${tabs[next].id}-tab`)
                  ?.focus();
              }
            }}
          >
            <Icon size={19} strokeWidth={1.65} />
            <span>{c(label)}</span>
          </button>
        ))}
      </nav>
      <div className="workbench-resource-layout">
        <nav
          className="workbench-resource-sidebar"
          aria-label={t("Resource categories")}
        >
          <span className="workbench-sidebar-heading">
            {tab === "media"
              ? t("My media")
              : c(tabs.find((item) => item.id === tab)!.label)}
          </span>
          {tab === "media" ? (
            <>
              <button
                aria-pressed={filter === "all"}
                onClick={() => setFilter("all")}
              >
                <FolderOpen size={14} />
                {t("All media")}
              </button>
              <button
                aria-pressed={filter === "video"}
                onClick={() => setFilter("video")}
              >
                <Clapperboard size={14} />
                {t("Video")}
              </button>
              <button
                aria-pressed={filter === "audio"}
                onClick={() => setFilter("audio")}
              >
                <AudioLines size={14} />
                {t("Audio")}
              </button>
              <button
                aria-pressed={filter === "image"}
                onClick={() => setFilter("image")}
              >
                <Image size={14} />
                {t("Images")}
              </button>
            </>
          ) : tab === "audio" ? (
            <>
              <button
                aria-pressed={audioSource === "catalog"}
                onClick={() => setAudioSource("catalog")}
              >
                {t("Sound library")}
              </button>
              <button
                aria-pressed={audioSource === "local"}
                onClick={() => setAudioSource("local")}
              >
                {t("Local audio")}
              </button>
            </>
          ) : tab === "graphic" ? (
            <>
              <button
                aria-pressed={graphicSource === "shapes"}
                onClick={() => setGraphicSource("shapes")}
              >
                {t("Basic shapes")}
              </button>
              <button
                aria-pressed={graphicSource === "flags"}
                onClick={() => setGraphicSource("flags")}
              >
                {t("Flags")}
              </button>
              <button
                aria-pressed={graphicSource === "brands"}
                onClick={() => setGraphicSource("brands")}
              >
                {t("Brands")}
              </button>
            </>
          ) : (
            <div className="workbench-sidebar-current">
              <span />
              {t(
                tab === "caption"
                  ? "Caption tools"
                  : tab === "text"
                    ? "Default text"
                    : tab === "effect"
                      ? "Scene effects"
                      : "Crossfade",
              )}
            </div>
          )}
          <span className="workbench-local-badge">
            <span />
            {t("Local workspace")}
          </span>
        </nav>
        <div
          className="workbench-resource-content"
          role="tabpanel"
          id="resource-active-view"
          aria-labelledby={`resource-${tab}-tab`}
        >
          <div hidden={!mediaVisible} className="workbench-resource-view">
            <MediaPanel filter={tab === "audio" ? "audio" : filter} />
          </div>
          <div
            ref={catalogView}
            hidden={!catalogVisible}
            className="workbench-resource-view workbench-catalog-view"
            onScroll={(event) => {
              if (catalogVisible)
                catalogPositions.current.set(
                  catalogKey,
                  event.currentTarget.scrollTop,
                );
            }}
          >
            <ResourceCatalogPanel
              active={catalogVisible}
              category={tab === "graphic" ? graphicSource : "sounds"}
            />
          </div>
          <div
            hidden={tab !== "text"}
            className="workbench-resource-view workbench-text-resources"
          >
            <div className="workbench-resource-title">
              <Type size={17} />
              <h3>{c("tabText")}</h3>
            </div>
            <div className="resource-text-grid">
              <TextActions card />
            </div>
            <h4>{t("Project fonts")}</h4>
            <div className="resource-text-grid">
              {scene?.head.project.fonts.map((font) => (
                <ProjectFontCard
                  key={font.id}
                  font={font}
                  active={tab === "text"}
                />
              ))}
            </div>
          </div>
          <div
            hidden={tab !== "caption"}
            className="workbench-resource-view workbench-caption-resources"
          >
            {captions}
          </div>
          <div
            hidden={tab !== "effect"}
            className="workbench-resource-view workbench-effect-resources"
          >
            <EffectsResources sequence={sequence} active={tab === "effect"} />
          </div>
          <div
            hidden={tab !== "transition"}
            className="workbench-resource-view"
          >
            <TransitionResources active={tab === "transition"} />
          </div>
        </div>
      </div>
    </>
  );
}
