import { useEffect, useRef, useState } from "react";
import { Film } from "lucide-react";
import type { ProjectRef } from "../model/types";
import type { EditorApi } from "../workspace/api";
import { t, useLocale } from "../shared/locale";

export function ProjectCover({
  api,
  project,
  refresh,
}: {
  api: EditorApi | null;
  project: ProjectRef;
  refresh: number;
}) {
  useLocale();
  const ref = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<{
    key: string;
    api: EditorApi;
    url?: string;
    failed?: boolean;
  } | null>(null);
  const sequenceId = api?.coverViews.read(project.id) ?? null;
  const key = JSON.stringify([
    project.id,
    project.revision,
    project.format,
    sequenceId,
    refresh,
  ]);
  useEffect(() => {
    const abort = new AbortController();
    let url: string | undefined,
      started = false;
    const start = () => {
      if (started || !api || abort.signal.aborted) return;
      started = true;
      void api.covers
        .request(project, abort.signal, sequenceId)
        .then((blob) => {
          if (abort.signal.aborted) return;
          url = blob ? URL.createObjectURL(blob) : undefined;
          setState({ key, api, url });
        })
        .catch(() => {
          if (!abort.signal.aborted) setState({ key, api, failed: true });
        });
    };
    const observer =
      typeof IntersectionObserver === "undefined"
        ? null
        : new IntersectionObserver(
            (entries) => {
              if (entries.some((entry) => entry.isIntersecting)) {
                observer?.disconnect();
                start();
              }
            },
            { rootMargin: "160px" },
          );
    if (observer && ref.current) observer.observe(ref.current);
    else start();
    return () => {
      observer?.disconnect();
      abort.abort();
      if (url) URL.revokeObjectURL(url);
    };
    // Fixed identity captures content; unrelated library refreshes do not restart work.
  }, [api, key]);
  const current = state?.key === key && state.api === api ? state : null;
  return (
    <div
      ref={ref}
      className="project-cover"
      title={
        current?.failed
          ? t("Cover unavailable. Refresh it from the project menu.")
          : undefined
      }
    >
      {current?.url ? (
        <img
          src={current.url}
          data-cover-revision={project.revision}
          alt={t("Cover of {name}", { name: project.name })}
          onError={() => api && setState({ key, api, failed: true })}
        />
      ) : (
        <>
          <Film size={40} />
          {current?.failed && <span>{t("Cover unavailable")}</span>}
        </>
      )}
    </div>
  );
}
