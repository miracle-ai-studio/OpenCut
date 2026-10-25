// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ResourceCatalogPanel } from "./catalog_resources-panel";
import { attachShortcutDispatcher } from "../../shortcuts/dispatcher";
import { defaultBindings } from "../../shortcuts/catalog";
import { ShortcutRegistry } from "../../shortcuts/registry";
import { preferences } from "../../shared/locale";
import type { ToolOutput } from "../../../lib/editor-sdk";
const mocks = vi.hoisted(() => ({ state: {} as unknown }));
vi.mock("../context", () => ({ useWorkspace: () => mocks.state }));
beforeEach(() => {
  preferences({ language: "en" });
  sessionStorage.clear();
});
afterEach(cleanup);
function fixture(available: boolean) {
  const catalog: ToolOutput<"opencut_catalog_resources_list"> = {
    sound_download_available: available,
    brands_available: false,
    sound_unavailable_reason: available
      ? null
      : "Public sound downloads are disabled and no local archive candidate is available.",
    entries: [
      {
        id: "kenney-ui-audio:click1",
        name: "click1",
        category: "sounds",
        keywords: ["click"],
        source_url: "https://kenney.nl/assets/ui-audio",
        source_sha256: "a".repeat(64),
        audio: null,
        width: null,
        height: null,
        license: {
          identifier: "CC0-1.0",
          source_url: null,
          notice: null,
          attribution: null,
        },
      },
    ],
  };
  const call = vi.fn(async () => catalog);
  mocks.state = {
    api: {
      connectionIdentity: JSON.stringify([
        "https://catalog.example",
        "principal",
        "credential-id",
        false,
      ]),
      authenticateConnection: async () => {},
      sdk: { call },
    },
    scene: {
      head: { project: { id: "project", assets: [] } },
      media: { attempt: null },
    },
    busy: false,
    controller: { getSnapshot: () => mocks.state, report: vi.fn() },
  };
  render(<ResourceCatalogPanel />);
  return { call };
}
it("allows the host's offline archive acquisition hint without claiming the candidate bytes are already verified", async () => {
  fixture(true);
  const acquire = await screen.findByRole("button", {
    name: /^Add to timeline ·/,
  });
  expect((acquire as HTMLButtonElement).disabled).toBe(false);
  expect(screen.getByRole("button", { name: /^Listen ·/ })).toBeTruthy();
});
it("keeps missing offline sources unavailable while local recipes remain separate", async () => {
  fixture(false);
  expect(
    (
      (await screen.findByRole("button", {
        name: /^Add to timeline ·/,
      })) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(screen.getByRole("status").textContent).toContain(
    "disabled and no local archive",
  );
  fireEvent.click(screen.getByText("Create a sound"));
  expect(
    (
      screen.getByRole("button", {
        name: "Create and add",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(false);
});

it("retains an independent search when switching sound and flag categories without any write", async () => {
  const f = fixture(true);
  await screen.findByRole("button", {name: /^Listen ·/});
  const search = screen.getByLabelText("Search catalog") as HTMLInputElement;
  fireEvent.change(search, {target: {value: "click"}});
  fireEvent.change(screen.getByLabelText("Category"), {target: {value: "flags"}});
  expect(search.value).toBe("");
  fireEvent.change(search, {target: {value: "China"}});
  fireEvent.change(screen.getByLabelText("Category"), {target: {value: "sounds"}});
  expect(search.value).toBe("click");
  fireEvent.change(screen.getByLabelText("Category"), {target: {value: "flags"}});
  expect(search.value).toBe("China");
  expect(f.call).toHaveBeenCalledOnce();
  expect(f.call).toHaveBeenCalledWith("opencut_catalog_resources_list", {}, expect.objectContaining({signal: expect.any(AbortSignal)}));
});

it("adds a resource through its original import, then directly applies the exact placement plan", async () => {
  const f = actionFixture();
  fireEvent.click(
    await screen.findByRole("button", { name: /^Add to timeline ·/ }),
  );
  await screen.findByText("Added to timeline");
  expect(f.importResource).toHaveBeenCalledOnce();
  expect(f.media.place).toHaveBeenCalledOnce();
  expect(f.media.apply).toHaveBeenCalledOnce();
  expect(f.media.place.mock.calls[0][2]).toEqual({
    projectId: "project",
    revision: "catalog-r1",
    sequenceId: "scene",
  });
  expect(f.media.place.mock.calls[0][0].start).toEqual({
    ticks: "1",
    timebase: 3,
  });
  expect(f.media.dismiss).toHaveBeenCalledOnce();
  expect(screen.queryByRole("dialog")).toBeNull();
});
it("retries an uncertain import with its exact key and does not invent a second placement intent", async () => {
  const f = actionFixture(true);
  fireEvent.click(
    await screen.findByRole("button", { name: /^Add to timeline ·/ }),
  );
  await screen.findByRole("alert");
  const original = structuredClone(f.importResource.mock.calls[0][0]);
  fireEvent.click(screen.getByRole("button", { name: "Retry original save" }));
  await screen.findByText(
    "Added to project. Choose Add to timeline to place it.",
  );
  expect(f.importResource).toHaveBeenCalledTimes(2);
  expect(f.importResource.mock.calls[1][0]).toEqual(original);
  expect(f.media.place).not.toHaveBeenCalled();
  expect(f.media.apply).not.toHaveBeenCalled();
});
it("does not apply a delayed placement after the active connection changed", async () => {
  const f = actionFixture(false, true);
  fireEvent.click(
    await screen.findByRole("button", { name: /^Add to timeline ·/ }),
  );
  await waitFor(() => expect(f.media.place).toHaveBeenCalledOnce());
  expect(f.media.apply).not.toHaveBeenCalled();
});

import { initialSnapshot } from "./fixture";
import type { CatalogImportRequest } from "./catalog_resources-recovery";
import type { MediaPlan, MediaRequest } from "./types";
function actionFixture(loseFirst = false, changeConnection = false) {
  const head = initialSnapshot();
  head.project.id = "project";
  const identity = JSON.stringify([
    "https://catalog.example",
    "principal",
    "credential-id",
    false,
  ]);
  const api = {
    connectionIdentity: identity,
    authenticateConnection: async () => {},
    sdk: {
      call: vi.fn(async () => ({
        sound_download_available: true,
        brands_available: false,
        sound_unavailable_reason: null,
        entries: [
          {
            id: "kenney-ui-audio:click1",
            name: "click1",
            category: "sounds",
            keywords: [],
            source_url: "https://kenney.nl/assets/ui-audio",
            source_sha256: "a".repeat(64),
        audio: null,
            width: null,
            height: null,
            license: {
              identifier: "CC0-1.0",
              source_url: null,
              notice: null,
              attribution: null,
            },
          },
        ],
      })),
    },
  };
  const state = {
    api,
    scene: {
      head,
      sequenceId: "scene",
      playhead: { ticks: "1", timebase: 3 },
      selectedId: null,
      media: {
        attempt: null as null | {
          request: { idempotency_key: string; sequence_id: string; bindings: MediaRequest["bindings"]; placements: MediaRequest["placements"] };
          plan: MediaPlan;
          phase: string;
          error: null;
        },
      },
    },
    busy: false,
    controller: {} as unknown,
  };
  const media = {
    place: vi.fn(
      async (
        _placement: Omit<MediaRequest["placements"][number], "element_id">,
        _track: unknown,
        _scope: unknown,
      ) => {
        const plan = {
          transaction: {
            project_id: "project",
            expected_revision: "catalog-r1",
            idempotency_key: "placement-key",
          },
        } as MediaPlan;
        state.scene.media.attempt = {
          request: { idempotency_key: "placement-key", sequence_id: "scene", bindings: [], placements: [{..._placement, element_id:"placed-sound"}] },
          plan,
          phase: "planned",
          error: null,
        };
        if (changeConnection)
          state.api = { ...api, connectionIdentity: "different" };
        return plan;
      },
    ),
    apply: vi.fn(async () => {
      state.scene.media.attempt!.phase = "saved";
      const clip = structuredClone(head.project.sequences[0].tracks[0].elements[0]);
      clip.id = "placed-sound";
      head.project.sequences[0].tracks[0].elements.push(clip);
    }),
    dismiss: vi.fn(() => {
      state.scene.media.attempt = null;
    }),
  };
  let calls = 0;
  const importResource = vi.fn(async (request: CatalogImportRequest) => {
    calls++;
    if (loseFirst && calls === 1) throw Error("Connection lost");
    head.project.assets.push({
      ...structuredClone(head.project.assets[0]),
      id: request.asset_id,
      kind: "audio",
      duration: { ticks: "1", timebase: 4 },
      origin: {
        kind: "online",
        provider: "kenney-ui-audio-v1",
        catalog_id: request.catalog_id,
        source_url: "https://kenney.nl/assets/ui-audio",
        license: {
          identifier: "CC0-1.0",
          source_url: null,
          notice: null,
          attribution: null,
        },
      },
    } as (typeof head.project.assets)[number]);
    head.revision = "catalog-r1";
    return {
      base_revision: request.expected_revision,
      revision: head.revision,
      transaction_id: "import-tx",
      created_ids: [request.asset_id],
      changed_ids: [],
      deleted_ids: [],
    };
  });
  state.controller = {
    getSnapshot: () => state,
    report: vi.fn(),
    scene: {
      importCatalogResource: importResource,
      refresh: async () => {},
      media,
    },
  };
  mocks.state = state;
  render(<ResourceCatalogPanel category="sounds" />);
  return { importResource, media };
}

it("gives sound rows and license summaries their keyboard keys before the real global capture dispatcher", async () => {
  const f = fixture(true);
  const play = await screen.findByRole("button", { name: /^Listen ·/ });
  const row = play.closest("article")!;
  row.parentElement!.classList.add("editor-app");
  const globalStart = vi.fn(), globalPlay = vi.fn(), localPlay = vi.fn();
  play.addEventListener("click", event => { event.preventDefault(); event.stopImmediatePropagation(); localPlay(); });
  const registry = new ShortcutRegistry();
  registry.register(Symbol(), () => ({ "goto-start": { run: globalStart }, "toggle-play": { run: globalPlay } }));
  const detach = attachShortcutDispatcher({ registry, bindings: defaultBindings, platform: "apple", onError: error => { throw error; } });
  const press = (target: Element, key: string, code: string) => {
    const event = new KeyboardEvent("keydown", { key, code, bubbles: true, cancelable: true });
    fireEvent(target, event);
    fireEvent.keyUp(target, { key, code });
    return event;
  };
  try {
    expect(press(row, "Enter", "Enter").defaultPrevented).toBe(true);
    expect(localPlay).toHaveBeenCalledOnce();
    const summary = row.querySelector("summary")!;
    expect(press(summary, "Enter", "Enter").defaultPrevented).toBe(false);
    expect(press(summary, " ", "Space").defaultPrevented).toBe(false);
    expect(globalStart).not.toHaveBeenCalled();
    expect(globalPlay).not.toHaveBeenCalled();
    expect(f.call).toHaveBeenCalledOnce();
  } finally { detach(); }
});
it("rejects a retargeted row drag that originated on an SVG inside a control", async () => {
  fixture(true);
  const play = await screen.findByRole("button", { name: /^Listen ·/ });
  const row = play.closest("article")!, svg = play.querySelector("svg")!;
  fireEvent.mouseDown(svg);
  const drag = new Event("dragstart", { bubbles: true, cancelable: true });
  fireEvent(row, drag);
  expect(drag.defaultPrevented).toBe(true);
  fireEvent.mouseDown(row.querySelector("strong")!);
  const bodyDrag = new Event("dragstart", { bubbles: true, cancelable: true });
  fireEvent(row, bodyDrag);
  expect(bodyDrag.defaultPrevented).toBe(false);
});
