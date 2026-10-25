import { afterEach, expect, it, vi } from "vitest";
import {
  OpenCutApiError,
  OpenCutTransportError,
} from "../../../lib/editor-sdk";
import { setupSourceAudio } from "./source-audio-fixture";
import type { SourceAudioRequest } from "./types";
afterEach(() => vi.restoreAllMocks());
it("applies the authoritative source-audio transaction unchanged and restores without deleting detached audio", async () => {
  const { controller, call, plans, current } = await setupSourceAudio();
  try {
    await controller.scene.sourceAudio.start("separate");
    expect(
      call.mock.calls.find(([name]) => name === "opencut_edit_apply")![1],
    ).toEqual(plans[0].transaction);
    expect(controller.getSnapshot().scene!.sourceAudio?.phase).toBe("saved");
    const separated = current().project.sequences[0].tracks[1].elements[0];
    const original =
      current().project.sequences[0].tracks[0].elements[0].content;
    expect(original.kind === "video" && original.source_audio_enabled).toBe(
      false,
    );
    await controller.scene.sourceAudio.start("restore");
    expect(current().project.sequences[0].tracks[1].elements[0]).toEqual(
      separated,
    );
    expect(plans[1].transaction.operations).toHaveLength(1);
    expect(
      call.mock.calls.filter(
        ([name]) => name === "opencut_edit_apply",
      )[1][1],
    ).toEqual(plans[1].transaction);
  } finally {
    controller.dispose();
  }
});
it("a view switch during planning fences apply and retries preserve all captured identities", async () => {
  const { controller, api, call, planFor } = await setupSourceAudio();
  try {
    let resolve!: (value: ReturnType<typeof planFor>) => void;
    let captured!: SourceAudioRequest;
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_project_source_audio_plan") {
        captured = structuredClone(input as SourceAudioRequest);
        return (await new Promise<ReturnType<typeof planFor>>((r) => {
          resolve = r;
        })) as never;
      }
      return previous(name, input, options);
    });
    const pending = controller.scene.sourceAudio.start("separate");
    controller.scene.returnToHead();
    resolve(planFor(captured));
    await expect(pending).rejects.toThrow("view changed");
    expect(
      call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
    ).toHaveLength(0);
    expect(controller.getSnapshot().scene!.sourceAudio!.request).toEqual(captured);
    controller.scene.select("video");
    await controller.scene.sourceAudio.retry();
    const input = call.mock.calls.find(
      ([name]) => name === "opencut_edit_apply",
    )![1];
    expect(input).toEqual(planFor(captured).transaction);
    expect(
      call.mock.calls.filter(
        ([name]) => name === "opencut_project_source_audio_plan",
      ),
    ).toHaveLength(1);
  } finally {
    controller.dispose();
  }
});
it.each([true, false])(
  "an unknown result reads head before explicit identical replay without creating another plan (initial commit=%s)",
  async (committed) => {
    const { controller, api, call, plans, commit, current } =
      await setupSourceAudio();
    try {
      const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
      let receipt: ReturnType<typeof commit> | null = null,
        first = true;
      vi.mocked(api.sdk.call).mockImplementation(
        async (name, input, options) => {
          if (name === "opencut_edit_apply") {
            if (first) {
              first = false;
              if (committed) receipt = commit(plans[0].transaction);
              throw new OpenCutTransportError(new Error("Lost response"));
            }
            receipt ??= commit(plans[0].transaction);
            return receipt as never;
          }
          return previous(name, input, options);
        },
      );
      await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
        "request could not",
      );
      expect(controller.getSnapshot().scene!.sourceAudio!.phase).toBe(
        "unconfirmed",
      );
      expect(controller.getSnapshot().scene!.sourceAudio!.headChecked).toBe(true);
      expect(controller.getSnapshot().scene!.sourceAudio!.receipt).toBeNull();
      expect(current().project.sequences[0].tracks).toHaveLength(
        committed ? 2 : 1,
      );
      expect(() => controller.scene.sourceAudio.dismiss()).toThrow(
        "saved outcome",
      );
      await expect(controller.scene.sourceAudio.start("restore")).rejects.toThrow(
        "previous source",
      );
      const checkpoint = call.mock.calls.length;
      await controller.scene.sourceAudio.retry();
      const calls = call.mock.calls.slice(checkpoint);
      expect(calls[0][0]).toBe("opencut_project_inspect");
      expect(
        calls.some(([name]) => name === "opencut_project_source_audio_plan"),
      ).toBe(false);
      const applies = call.mock.calls.filter(
        ([name]) => name === "opencut_edit_apply",
      );
      expect(applies).toHaveLength(2);
      expect(applies[1][1]).toEqual(applies[0][1]);
      expect(
        call.mock.calls.filter(
          ([name]) => name === "opencut_project_source_audio_plan",
        ),
      ).toHaveLength(1);
      expect(current().project.sequences[0].tracks).toHaveLength(2);
      expect(controller.getSnapshot().scene!.sourceAudio!.phase).toBe("saved");
    } finally {
      controller.dispose();
    }
  },
);
it("stale head and historical or locked/absent/unknown media cannot silently submit", async () => {
  const { controller, current, api, call, planFor } = await setupSourceAudio();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_project_source_audio_plan") {
        const plan = planFor(input as SourceAudioRequest);
        current().revision = "remote-head";
        await controller.scene.refresh();
        return plan as never;
      }
      return previous(name, input, options);
    });
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "view changed",
    );
    expect(
      call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
    ).toHaveLength(0);
    controller.scene.sourceAudio.dismiss();
    await controller.scene.inspectRevision("r0");
    controller.scene.select("video");
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "current revision",
    );
    controller.scene.returnToHead();
    controller.scene.select("video");
    for (const status of ["unknown", "absent"] as const) {
      current().project.assets[0].metadata.audio = { status };
      await controller.scene.refresh();
      await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
        status === "unknown" ? "must be checked" : "no source audio",
      );
    }
    current().project.assets[0].metadata.audio = { status: "present" };
    current().project.sequences[0].tracks[0].locked = true;
    await controller.scene.refresh();
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "locked",
    );
  } finally {
    controller.dispose();
  }
});
it("revoked apply is visible and the same captured public transaction remains available for retry", async () => {
  const { controller, api, plans, call } = await setupSourceAudio();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_apply")
        throw new OpenCutApiError(
          "PERMISSION_DENIED",
          "Editing permission was revoked.",
          null,
          403,
          "req",
          [],
        );
      return previous(name, input, options);
    });
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "revoked",
    );
    expect(controller.getSnapshot().scene!.sourceAudio!.error).toContain(
      "revoked",
    );
    expect(controller.getSnapshot().scene!.sourceAudio!.plan!.transaction).toEqual(
      plans[0].transaction,
    );
    await expect(controller.scene.sourceAudio.retry()).rejects.toThrow("revoked");
    const inputs = call.mock.calls
      .filter(([name]) => name === "opencut_edit_apply")
      .map(([, input]) => input);
    expect(inputs[0]).toEqual(inputs[1]);
  } finally {
    controller.dispose();
  }
});

it("same-connection project reopen retains an unresolved transaction and its original key", async () => {
  const { controller, api, call, plans, ref } = await setupSourceAudio();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let first = true;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_apply" && first) {
        first = false;
        throw new OpenCutTransportError(new Error("Lost response"));
      }
      return previous(name, input, options);
    });
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "request could not",
    );
    const captured = structuredClone(controller.getSnapshot().scene!.sourceAudio!);
    await controller.open(ref());
    expect(controller.getSnapshot().scene!.sourceAudio!.request).toEqual(
      captured.request,
    );
    expect(controller.getSnapshot().scene!.sourceAudio!.plan).toEqual(
      captured.plan,
    );
    expect(controller.getSnapshot().scene!.sourceAudio!.phase).toBe("unconfirmed");
    controller.scene.select("video");
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "previous source",
    );
    await controller.scene.sourceAudio.retry();
    expect(
      call.mock.calls.filter(
        ([name]) => name === "opencut_project_source_audio_plan",
      ),
    ).toHaveLength(1);
    expect(
      call.mock.calls
        .filter(([name]) => name === "opencut_edit_apply")
        .map(([, input]) => input),
    ).toEqual([plans[0].transaction, plans[0].transaction]);
  } finally {
    controller.dispose();
  }
});
it("an identical unknown replay conflict resolves the pending identity without automatically replanning", async () => {
  const { controller, api, call, current } = await setupSourceAudio();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let first = true;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_apply") {
        if (first) {
          first = false;
          throw new OpenCutTransportError(new Error("Lost response"));
        }
        throw new OpenCutApiError(
          "REVISION_CONFLICT",
          "Project has changed.",
          null,
          409,
          "req",
          [],
        );
      }
      return previous(name, input, options);
    });
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "request could not",
    );
    current().revision = "other-author-head";
    await expect(controller.scene.sourceAudio.retry()).rejects.toThrow(
      "original operation was not saved",
    );
    expect(controller.getSnapshot().scene!.head.revision).toBe(
      "other-author-head",
    );
    expect(controller.getSnapshot().scene!.sourceAudio).toBeNull();
    expect(
      call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
    ).toHaveLength(2);
    expect(
      call.mock.calls.filter(
        ([name]) => name === "opencut_project_source_audio_plan",
      ),
    ).toHaveLength(1);
  } finally {
    controller.dispose();
  }
});
it.each(["PERMISSION_DENIED", "EDIT_HELD"])(
  "a rejected replay with %s cannot disprove an earlier unknown result",
  async (code) => {
    const { controller, api, call, plans } = await setupSourceAudio();
    try {
      const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
      let first = true;
      vi.mocked(api.sdk.call).mockImplementation(
        async (name, input, options) => {
          if (name === "opencut_edit_apply") {
            if (first) {
              first = false;
              throw new OpenCutTransportError(new Error("Lost response"));
            }
            throw new OpenCutApiError(
              code,
              "Editing is unavailable.",
              null,
              403,
              "req",
              [],
            );
          }
          return previous(name, input, options);
        },
      );
      await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
        "request could not",
      );
      await expect(controller.scene.sourceAudio.retry()).rejects.toThrow(
        "unavailable",
      );
      expect(controller.getSnapshot().scene!.sourceAudio!.phase).toBe(
        "unconfirmed",
      );
      expect(() => controller.scene.sourceAudio.dismiss()).toThrow(
        "saved outcome",
      );
      expect(
        call.mock.calls
          .filter(([name]) => name === "opencut_edit_apply")
          .map(([, input]) => input),
      ).toEqual([plans[0].transaction, plans[0].transaction]);
    } finally {
      controller.dispose();
    }
  },
);

it("a late plan after route reopen updates only the retained command and cannot commit or surface an old-view error", async () => {
  const { controller, api, call, planFor, ref } = await setupSourceAudio();
  try {
    let resolve!: (value: ReturnType<typeof planFor>) => void;
    let captured!: SourceAudioRequest;
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_project_source_audio_plan") {
        captured = structuredClone(input as SourceAudioRequest);
        return (await new Promise<ReturnType<typeof planFor>>((r) => {
          resolve = r;
        })) as never;
      }
      return previous(name, input, options);
    });
    const pending = controller.scene.sourceAudio.start("separate");
    await controller.open(ref());
    resolve(planFor(captured));
    const error = await pending.catch((e) => e);
    expect(error).toBeInstanceOf(DOMException);
    expect(error.name).toBe("AbortError");
    controller.report(error);
    expect(controller.getSnapshot().error).toBeNull();
    expect(controller.getSnapshot().scene!.sourceAudio!.request).toEqual(captured);
    expect(controller.getSnapshot().scene!.sourceAudio!.plan!.transaction).toEqual(
      planFor(captured).transaction,
    );
    expect(
      call.mock.calls.filter(([name]) => name === "opencut_edit_apply"),
    ).toHaveLength(0);
    controller.scene.select("video");
    await controller.scene.sourceAudio.retry();
    expect(
      call.mock.calls.filter(
        ([name]) => name === "opencut_project_source_audio_plan",
      ),
    ).toHaveLength(1);
  } finally {
    controller.dispose();
  }
});
it("the shared write fence cannot downgrade an unknown source-audio outcome", async () => {
  const { controller, api, current } = await setupSourceAudio();
  try {
    const previous = vi.mocked(api.sdk.call).getMockImplementation()!;
    let first = true,
      release!: (value: unknown) => void;
    vi.mocked(api.sdk.call).mockImplementation(async (name, input, options) => {
      if (name === "opencut_edit_plan")
        return (await new Promise<unknown>((r) => {
          release = r;
        })) as never;
      if (name === "opencut_edit_apply" && first) {
        first = false;
        throw new OpenCutTransportError(new Error("Lost response"));
      }
      return previous(name, input, options);
    });
    await expect(controller.scene.sourceAudio.start("separate")).rejects.toThrow(
      "request could not",
    );
    const write = controller.scene.write("Other edit", []);
    await expect(controller.scene.sourceAudio.retry()).rejects.toThrow(
      "current edit",
    );
    expect(controller.getSnapshot().scene!.sourceAudio!.phase).toBe("unconfirmed");
    expect(() => controller.scene.sourceAudio.dismiss()).toThrow("saved outcome");
    release({ project: structuredClone(current().project) });
    await expect(write).rejects.toThrow("Resolve the original edit");
    expect(controller.getSnapshot().scene!.recovery!.record!.context.intent.request.idempotency_key)
      .toBe(controller.getSnapshot().scene!.sourceAudio!.plan!.transaction.idempotency_key);
  } finally {
    controller.dispose();
  }
});
