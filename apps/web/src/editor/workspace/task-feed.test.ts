import { describe, it, expect, vi } from "vitest";
import { TaskFeed } from "./task-feed";
import type { OpenCutClient } from "../../lib/editor-sdk";
const job = (id: string, state = "running") => ({
  job_id: id,
  state,
  target: { project_id: "project" },
});
describe("durable task feed", () => {
  it("reads every bootstrap page then polls changes without rescanning history", async () => {
    let changed = false;
    const call = vi.fn(
      async (
        name: string,
        input: { cursor?: string | null; job_id?: string },
      ) => {
        if (name === "opencut_job_changes")
          return {
            events: changed ? [{ job_id: "new" }] : [],
            next_cursor: changed ? "after-change" : "initial",
          };
        if (name === "opencut_job_query")
          return input.cursor
            ? { jobs: [job("new")], next_cursor: null }
            : { jobs: [job("old")], next_cursor: "page-two" };
        if (name === "opencut_job_view") return job(input.job_id!, "succeeded");
        throw new Error(name);
      },
    );
    const feed = new TaskFeed({ call } as unknown as OpenCutClient);
    expect((await feed.read("project")).jobs.map((job) => job.job_id)).toEqual([
      "new",
      "old",
    ]);
    changed = true;
    expect((await feed.read("project")).jobs[0].state).toBe("succeeded");
    expect(
      call.mock.calls.filter(([name]) => name === "opencut_job_query"),
    ).toHaveLength(2);
    expect(
      [...call.mock.calls]
        .reverse()
        .find(([name]) => name === "opencut_job_changes")?.[1].cursor,
    ).toBe("initial");
  });
  it("does not advance past failed task refreshes", async () => {
    let initialized = false,
      failed = false;
    const cursors: unknown[] = [];
    const call = vi.fn(
      async (name: string, input: { cursor?: string | null }) => {
        if (name === "opencut_job_query") {
          initialized = true;
          return { jobs: [], next_cursor: null };
        }
        if (name === "opencut_job_changes") {
          cursors.push(input.cursor);
          return {
            events: initialized ? [{ job_id: "new" }] : [],
            next_cursor: initialized ? "next" : "start",
          };
        }
        if (name === "opencut_job_view") {
          if (!failed) {
            failed = true;
            throw new Error("network");
          }
          return job("new");
        }
      },
    );
    const feed = new TaskFeed({ call } as unknown as OpenCutClient);
    await feed.read("project");
    await expect(feed.read("project")).rejects.toThrow("network");
    expect((await feed.read("project")).jobs).toHaveLength(1);
    expect(cursors).toEqual([null, "start", "start"]);
  });
});
