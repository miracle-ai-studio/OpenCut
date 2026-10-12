import { useEffect, useRef, useState } from "react";
import { Button, Field, Modal } from "../shared/ui";
import { t, useLocale } from "../shared/locale";
import { useWorkspace } from "../workspace/context";
import {
  captureClassic,
  discoverClassic,
  readClassicFile,
} from "../workspace/classic/source";
import type { ClassicSource } from "../workspace/classic/source";
import { readRecoveryBundle } from "../workspace/classic/bundle";
import { fileDigest } from "../workspace/classic/identity";
import { classicExporterUrl } from "../workspace/classic/exporter.generated";
import {
  cleanupRecovery,
  findRecovered,
  prepareRecovery,
  publishRecovery,
  reviewRecovery,
} from "../workspace/classic/recovery";
import type {
  PreparedRecovery,
  RecoveryProgress,
  RecoveryReview,
} from "../workspace/classic/recovery";
export function MigrateClassic({
  onClose,
  onOpen,
}: {
  onClose: () => void;
  onOpen: () => void;
}) {
  useLocale();
  const { api, controller } = useWorkspace();
  const [sources, setSources] = useState<ClassicSource[]>([]),
    [selected, setSelected] = useState("");
  const [bundle, setBundle] = useState<Awaited<
    ReturnType<typeof readRecoveryBundle>
  > | null>(null);
  const [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [progress, setProgress] = useState<RecoveryProgress | null>(null),
    [prepared, setPrepared] = useState<PreparedRecovery | null>(null);
  const [review, setReview] = useState<RecoveryReview | null>(null),
    [accepted, setAccepted] = useState(false);
  const [completed, setCompleted] = useState<string | null>(null),
    [warning, setWarning] = useState<string | null>(null);
  const [exporter, setExporter] = useState("");
  const bookmark = useRef<HTMLAnchorElement>(null),
    abort = useRef<AbortController | null>(null),
    epoch = useRef(0);
  const close = () => {
    epoch.current++;
    abort.current?.abort();
    onClose();
    void controller.refreshProjects().catch(() => {});
  };
  useEffect(() => {
    const lifetime = new AbortController();
    let active = true;
    void discoverClassic(lifetime.signal)
      .then((rows) => {
        if (active) {
          setSources(rows);
          setSelected(rows.find((r) => !r.error)?.id ?? "");
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    void fetch(classicExporterUrl, { signal: lifetime.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error("Old-site exporter is unavailable");
        const code = await response.text();
        if (active) setExporter(code);
      })
      .catch((e) => {
        if (active) setWarning(String(e));
      });
    return () => {
      active = false;
      epoch.current++;
      lifetime.abort();
      abort.current?.abort();
    };
  }, []);
  useEffect(() => {
    if (bookmark.current && exporter)
      bookmark.current.href = `javascript:${encodeURIComponent(exporter)}`;
  }, [exporter]);
  useEffect(() => {
    setBusy(false);
    setReview(null);
    setPrepared(null);
    setCompleted(null);
    setAccepted(false);
    setProgress(null);
    return () => {
      epoch.current++;
      abort.current?.abort();
    };
  }, [api]);
  const reset = () => {
    setReview(null);
    setPrepared(null);
    setCompleted(null);
    setAccepted(false);
    setError("");
    setWarning(null);
    setProgress(null);
  };
  const run = async (
    action: (signal: AbortSignal, generation: number) => Promise<void>,
  ) => {
    abort.current?.abort();
    const operation = new AbortController();
    abort.current = operation;
    const generation = ++epoch.current;
    setBusy(true);
    setError("");
    try {
      await action(operation.signal, generation);
    } catch (e) {
      if (epoch.current === generation && !operation.signal.aborted)
        setError(String(e));
    } finally {
      if (epoch.current === generation) setBusy(false);
    }
  };
  const update = (generation: number, value: RecoveryProgress) => {
    if (epoch.current === generation) setProgress(value);
  };
  const prepare = () =>
    void run(async (signal, generation) => {
      if (!api) throw new Error("Connect to the local service first");
      setReview(null);
      setCompleted(null);
      setAccepted(false);
      let input = bundle;
      if (!input) {
        const snapshot = await captureClassic(selected, signal),
          media = [];
        for (const [index, row] of snapshot.media.entries()) {
          update(generation, {
            phase: "checking",
            name: String(row.name ?? row.id),
            index,
            count: snapshot.media.length,
          });
          const file = await readClassicFile(snapshot.sourceId, row);
          media.push({
            id: String(row.id),
            file,
            sha256: await fileDigest(file, signal),
          });
        }
        input = { snapshot, media };
        const current = await captureClassic(snapshot.sourceId, signal);
        if (JSON.stringify(current) !== JSON.stringify(snapshot))
          throw new Error(
            "Classic project changed during recovery. Retry to capture its current data.",
          );
      }
      const item = await prepareRecovery(
        input.snapshot,
        input.media,
        signal,
        (p) => update(generation, p),
      );
      if (epoch.current !== generation) return;
      setPrepared(item);
      const previous = await findRecovered(api.sdk, item, signal);
      if (previous) {
        setWarning(await cleanupRecovery(api.sdk, item));
        if (epoch.current === generation) {
          setCompleted(previous);
          setProgress(null);
          await controller.refreshProjects();
        }
        return;
      }
      const result = await reviewRecovery(api.sdk, item, signal, (p) =>
        update(generation, p),
      );
      if (epoch.current === generation) setReview(result);
    });
  const publish = () =>
    void run(async (signal, generation) => {
      if (!api || !review || (!accepted && review.analysis.losses.length))
        return;
      update(generation, { phase: "publishing" });
      const id = await publishRecovery(api.sdk, review, signal);
      const cleanup = await cleanupRecovery(api.sdk, review.prepared);
      if (controller.getSnapshot().api === api)
        await controller.refreshProjects();
      if (epoch.current === generation) {
        setCompleted(id);
        setWarning(cleanup);
        setProgress(null);
        setReview(null);
      }
    });
  const pause = () => {
    epoch.current++;
    abort.current?.abort();
    setBusy(false);
    setProgress(null);
    setError("");
    setWarning(t("Recovery paused. Uploaded files remain ready to resume."));
  };
  const open = async () => {
    await controller.refreshProjects();
    const project = controller
      .getSnapshot()
      .projects.find((p) => p.id === completed);
    if (!project) throw new Error("Recovered project is no longer available");
    await controller.open(project);
    close();
    onOpen();
  };
  const phases = {
    checking: "Checking original files…",
    uploading: "Uploading original files…",
    preparing: "Preparing recovered media…",
    review: "Review recovery",
    publishing: "Publishing recovered project…",
  };
  return (
    <Modal
      title={t("Recover Classic projects")}
      onClose={close}
      className="classic-recovery"
    >
      <p>
        {t(
          "Recover old browser projects into this workspace. Original databases and media are preserved.",
        )}
      </p>
      {loading ? (
        <p role="status">{t("Looking for Classic projects…")}</p>
      ) : (
        <>
          <Field label={t("Classic project on this site")}>
            <select
              disabled={busy}
              value={selected}
              onChange={(e) => {
                reset();
                setBundle(null);
                setSelected(e.target.value);
              }}
            >
              <option value="">{t("Choose an old project")}</option>
              {sources.map((source) => (
                <option
                  key={source.id}
                  value={source.id}
                  disabled={!!source.error}
                >
                  {source.name} · v{source.version}
                  {source.error ? ` · ${t("Damaged")}` : ""}
                </option>
              ))}
            </select>
          </Field>
          {!sources.length && (
            <p>{t("No Classic projects were found on this origin.")}</p>
          )}
        </>
      )}
      <details className="recovery-bridge" open={!sources.length}>
        <summary>{t("Projects saved on another site")}</summary>
        <ol>
          <li>
            {t("Drag the exporter link to your bookmarks bar.")}{" "}
            {exporter && (
              <a
                ref={bookmark}
                className="recovery-bookmark"
                onClick={(e) => e.preventDefault()}
              >
                {t("Export old Classic projects")}
              </a>
            )}
          </li>
          <li>
            {t(
              "Open the original Classic site and run that bookmark to export a recovery bundle containing its projects and media.",
            )}
          </li>
          <li>{t("Return here and choose the exported recovery bundle.")}</li>
        </ol>
        <p>
          {t(
            "The exporter runs only on the old site, reads locally and leaves the original data intact. A project JSON alone cannot recover missing media.",
          )}
        </p>
        <Field label={t("Classic recovery bundle")}>
          <input
            type="file"
            accept=".occlassic"
            disabled={busy}
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (!file) return;
              reset();
              setBundle(null);
              setSelected("");
              void run(async (signal, generation) => {
                const input = await readRecoveryBundle(
                  file,
                  signal,
                  (name, index, count) =>
                    update(generation, {
                      phase: "checking",
                      name,
                      index,
                      count,
                    }),
                );
                if (epoch.current === generation) {
                  setBundle(input);
                  setSelected("");
                  setProgress(null);
                }
              });
            }}
          />
        </Field>
        {bundle && (
          <p>
            {t("Recovery bundle from {origin}", {
              origin: bundle.snapshot.origin,
            })}{" "}
            · {bundle.media.length} {t("media files")}
          </p>
        )}
      </details>
      {progress && (
        <div className="recovery-progress" role="status">
          <strong>{t(phases[progress.phase])}</strong>
          {progress.name && (
            <p>
              {progress.count
                ? `${(progress.index ?? 0) + 1}/${progress.count} · `
                : ""}
              {progress.name}
            </p>
          )}
          {progress.percent !== undefined && (
            <progress
              value={progress.percent}
              max={100}
              aria-label={t("Recovery upload progress")}
            />
          )}
        </div>
      )}
      {error && <p role="alert">{t(error.replace(/^Error: /, ""))}</p>}
      {warning && <p role="status">{t(warning)}</p>}
      {review && (
        <section aria-label={t("Review recovery")}>
          <h3>{review.prepared.name}</h3>
          <p>
            {t("{count} original library files are ready.", {
              count: review.prepared.media.length,
            })}
          </p>
          {!!review.analysis.blockers.length && (
            <>
              <p role="alert">
                {t(
                  "This project contains unsupported content. Nothing has been published; the original is preserved.",
                )}
              </p>
              <ul className="recovery-findings">
                {review.analysis.blockers.map((item, i) => (
                  <li key={i}>
                    <code>{item.path}</code> · {item.code}
                    <p>{item.detail}</p>
                  </li>
                ))}
              </ul>
            </>
          )}
          {!!review.analysis.losses.length && (
            <>
              <p>
                {t(
                  "Review what will be excluded from the new project. Original data stays available.",
                )}
              </p>
              <details>
                <summary>
                  {t("{count} recovery differences", {
                    count: review.analysis.losses.length,
                  })}
                </summary>
                <ul className="recovery-findings">
                  {review.analysis.losses.map((item, i) => (
                    <li key={i}>
                      <code>{item.path}</code> · {item.code}
                      <p>{item.detail}</p>
                    </li>
                  ))}
                </ul>
              </details>
              <label className="recovery-consent">
                <input
                  type="checkbox"
                  checked={accepted}
                  disabled={busy}
                  onChange={(e) => setAccepted(e.target.checked)}
                />
                {t("I accept these listed differences for the recovered copy.")}
              </label>
            </>
          )}
        </section>
      )}
      {completed && (
        <p role="status">
          {t("Recovery complete. Your old project remains untouched.")}
        </p>
      )}
      <div className="modal-footer">
        {busy ? (
          <Button disabled={progress?.phase === "publishing"} onClick={pause}>
            {t("Pause recovery")}
          </Button>
        ) : (
          <>
            {completed ? (
              <Button
                className="primary"
                onClick={() => void open().catch((e) => setError(String(e)))}
              >
                {t("Open recovered project")}
              </Button>
            ) : (
              <>
                <Button
                  disabled={!api || (!selected && !bundle) || loading}
                  onClick={prepare}
                >
                  {review || error || prepared
                    ? t("Retry recovery")
                    : t("Prepare recovery")}
                </Button>
                {review && (
                  <Button
                    className="primary"
                    disabled={
                      !!review.analysis.blockers.length ||
                      !review.analysis.proposed_project ||
                      (!!review.analysis.losses.length && !accepted)
                    }
                    onClick={publish}
                  >
                    {t("Recover project")}
                  </Button>
                )}
              </>
            )}
          </>
        )}
        <Button onClick={close}>{t("Close")}</Button>
      </div>
    </Modal>
  );
}
