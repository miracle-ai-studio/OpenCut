import { OpenCutApiError } from "../../lib/editor-sdk";
import type {
  OpenCutClient,
  Job,
  ToolOutput,
} from "../../lib/editor-sdk";
/** Poll changes after a single paged bootstrap; never rescan all task history per tick. */
export class TaskFeed {
  private jobs = new Map<string, Job>();
  private cursor: string | null = null;
  private initialized = false;
  private pending: Promise<void> | null = null;
  constructor(private sdk: OpenCutClient) {}
  private async changes(cursor: string | null, signal?: AbortSignal) {
    const ids = new Set<string>();
    for (;;) {
      const page: ToolOutput<"opencut_job_changes"> = await this.sdk.call(
        "opencut_job_changes",
        { cursor, job_id: null, limit: 100 },
        { signal },
      );
      for (const event of page.events) ids.add(event.job_id);
      cursor = page.next_cursor ?? cursor;
      if (page.events.length < 100) return { ids, cursor };
    }
  }
  private async refresh(signal?: AbortSignal) {
    const changes = await this.changes(this.cursor, signal);
    if (!this.initialized) {
      let cursor: string | null = null;
      const jobs = new Map<string, Job>();
      do {
        const page: ToolOutput<"opencut_job_query"> = await this.sdk.call(
          "opencut_job_query",
          { cursor, limit: 100 },
          { signal },
        );
        for (const job of page.jobs) jobs.set(job.job_id, job);
        cursor = page.next_cursor ?? null;
      } while (cursor);
      this.jobs = jobs;
      this.initialized = true;
    } else {
      const ids = [...changes.ids],
        updates: Job[] = [];
      for (let start = 0; start < ids.length; start += 4) {
        const batch = await Promise.all(
          ids.slice(start, start + 4).map(async (job_id) => {
            try {
              return await this.sdk.call(
                "opencut_job_view",
                { job_id },
                { signal },
              );
            } catch (error) {
              if (
                error instanceof OpenCutApiError &&
                error.code === "NOT_FOUND"
              )
                return null;
              throw error;
            }
          }),
        );
        updates.push(...batch.filter((job): job is Job => job !== null));
      }
      for (const job of updates) this.jobs.set(job.job_id, job);
    }
    this.cursor = changes.cursor;
  }
  async read(projectId: string, signal?: AbortSignal) {
    if (!this.pending)
      this.pending = this.refresh(signal).finally(() => {
        this.pending = null;
      });
    await this.pending;
    return {
      jobs: [...this.jobs.values()]
        .filter(
          (job) =>
            job.target?.project_id === projectId ||
            job.resource_target?.project_id === projectId,
        )
        .reverse(),
      next_cursor: null,
    };
  }
}
