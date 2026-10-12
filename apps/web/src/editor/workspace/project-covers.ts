import type { ProjectRef } from "../model/types";
export type CoverRequest = ProjectRef & { coverSequenceId: string | null };

type Work = {
  abort: AbortController;
  promise: Promise<Blob | null>;
  users: number;
};
export const coverKey = (
  project: ProjectRef,
  sequenceId: string | null = null,
) => JSON.stringify([project.id, project.revision, project.format, sequenceId]);

// Derived view only. Each EditorApi owns a bounded host-session cache, without
// project documents, credentials, native paths or persistent browser storage.
export class ProjectCovers {
  private completed = new Map<string, Blob | null>();
  private work = new Map<string, Work>();
  private active = 0;
  private queue: Array<() => void> = [];
  private held = new Set<string>();
  constructor(
    private load: (
      project: CoverRequest,
      signal: AbortSignal,
    ) => Promise<Blob | null>,
  ) {}
  private async slot(signal: AbortSignal) {
    signal.throwIfAborted();
    if (this.active < 2) {
      this.active++;
      return;
    }
    await new Promise<void>((resolve, reject) => {
      const start = () => {
        signal.removeEventListener("abort", abort);
        this.active++;
        resolve();
      };
      const abort = () => {
        this.queue = this.queue.filter((f) => f !== start);
        reject(signal.reason);
      };
      this.queue.push(start);
      signal.addEventListener("abort", abort, { once: true });
    });
  }
  request(
    project: ProjectRef,
    signal: AbortSignal,
    sequenceId: string | null = null,
  ): Promise<Blob | null> {
    signal.throwIfAborted();
    if (this.held.has(project.id))
      return Promise.reject(
        new DOMException("Project cover suspended", "AbortError"),
      );
    const key = coverKey(project, sequenceId);
    if (this.completed.has(key)) {
      const blob = this.completed.get(key)!;
      this.completed.delete(key);
      this.completed.set(key, blob);
      return Promise.resolve(blob);
    }
    let work = this.work.get(key);
    if (!work || work.abort.signal.aborted) {
      const abort = new AbortController();
      work = { abort, users: 0, promise: Promise.resolve(null) };
      const created = work;
      const captured = { ...project, coverSequenceId: sequenceId };
      this.work.set(key, work);
      work.promise = (async () => {
        let held = false;
        try {
          await this.slot(abort.signal);
          held = true;
          abort.signal.throwIfAborted();
          const blob = await this.load(captured, abort.signal);
          abort.signal.throwIfAborted();
          if (blob && (blob.type !== "image/png" || blob.size > 1024 * 1024))
            throw new Error("Invalid project cover");
          this.completed.set(key, blob);
          while (this.completed.size > 32)
            this.completed.delete(this.completed.keys().next().value!);
          return blob;
        } finally {
          if (held) {
            this.active--;
            this.queue.shift()?.();
          }
          if (this.work.get(key) === created) this.work.delete(key);
        }
      })();
    }
    const shared = work;
    shared.users++;
    return new Promise((resolve, reject) => {
      let done = false;
      const finish = () => {
        if (done) return false;
        done = true;
        signal.removeEventListener("abort", cancel);
        if (--shared.users === 0) shared.abort.abort();
        return true;
      };
      const cancel = () => {
        if (finish()) reject(signal.reason);
      };
      signal.addEventListener("abort", cancel, { once: true });
      shared.promise.then(
        (blob) => {
          if (finish()) resolve(blob);
        },
        (error) => {
          if (finish()) reject(error);
        },
      );
    });
  }
  invalidate(projectId: string) {
    for (const key of this.completed.keys())
      if (JSON.parse(key)[0] === projectId) this.completed.delete(key);
    for (const [key, work] of this.work)
      if (JSON.parse(key)[0] === projectId) work.abort.abort();
  }
  retain(projects: ProjectRef[]) {
    const identities = new Set(
      projects.map((p) => JSON.stringify([p.id, p.revision, p.format])),
    );
    const kept = (key: string) =>
      identities.has(JSON.stringify(JSON.parse(key).slice(0, 3)));
    for (const key of this.completed.keys())
      if (!kept(key)) this.completed.delete(key);
    for (const [key, work] of this.work) if (!kept(key)) work.abort.abort();
  }
  dispose() {
    this.completed.clear();
    for (const work of this.work.values()) work.abort.abort();
  }
  hold(projectIds: string[]) {
    for (const id of projectIds) this.held.add(id);
    return () => {
      for (const id of projectIds) this.held.delete(id);
    };
  }
  async quiesce(projectIds: string[]) {
    const ids = new Set(projectIds);
    const work = [...this.work]
      .filter(([key]) => ids.has(JSON.parse(key)[0]))
      .map(([, work]) => work);
    for (const entry of work) entry.abort.abort();
    await Promise.all(
      work.map((entry) =>
        entry.promise.catch((error) => {
          if (error !== entry.abort.signal.reason) throw error;
        }),
      ),
    );
  }
}

export async function thumbnail(blob: Blob, signal: AbortSignal) {
  signal.throwIfAborted();
  const bitmap = await createImageBitmap(blob);
  try {
    signal.throwIfAborted();
    const scale = Math.min(1, 320 / bitmap.width, 180 / bitmap.height);
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Project cover unavailable");
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    const result = await new Promise<Blob>((resolve, reject) =>
      canvas.toBlob(
        (value) =>
          value
            ? resolve(value)
            : reject(new Error("Project cover unavailable")),
        "image/png",
      ),
    );
    signal.throwIfAborted();
    return result;
  } finally {
    bitmap.close();
  }
}
