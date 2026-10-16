// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { V4ResourceCatalogPanel } from "./catalog_resources_v1-panel";
import { preferences } from "../../shared/locale";
import type { ToolOutput } from "../../../lib/editor-sdk";
const mocks = vi.hoisted(() => ({ state: {} as unknown }));
vi.mock("../context", () => ({ useWorkspace: () => mocks.state }));
beforeEach(() => { preferences({ language: "en" }); sessionStorage.clear(); });
afterEach(cleanup);
function fixture(available: boolean) {
  const catalog: ToolOutput<"opencut_catalog_resources_v1_list"> = {
    schema_version: 1, sound_download_available: available, brands_available: false,
    sound_unavailable_reason: available ? null : "Public sound downloads are disabled and no local archive candidate is available.",
    entries: [{ id: "kenney-ui-audio:click1", name: "click1", category: "sounds", keywords: ["click"],
      source_url: "https://kenney.nl/assets/ui-audio", source_sha256: "a".repeat(64), width: null, height: null,
      license: { identifier: "CC0-1.0", source_url: null, notice: null, attribution: null } }],
  };
  const call = vi.fn(async () => catalog);
  mocks.state = { api: { connectionIdentity: JSON.stringify(["https://catalog.example", "principal", "credential-id", false]), authenticateConnection: async () => {}, sdk: { call } },
    v4: { head: { project: { id: "project", assets: [] } }, media: { attempt: null } }, busy: false,
    controller: { getSnapshot: () => mocks.state, report: vi.fn() } };
  render(<V4ResourceCatalogPanel />);
  return { call };
}
it("allows the host's offline archive acquisition hint without claiming the candidate bytes are already verified", async () => {
  fixture(true);
  const acquire = await screen.findByRole("button", { name: "Acquire and preview" });
  expect((acquire as HTMLButtonElement).disabled).toBe(false);
  expect(screen.getByText(/Sources are verified when acquired/)).toBeTruthy();
});
it("keeps missing offline sources unavailable while local recipes remain separate", async () => {
  fixture(false);
  expect((await screen.findByRole("button", { name: "Acquire and preview" }) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole("status").textContent).toContain("disabled and no local archive");
  expect((screen.getByRole("button", { name: "Generate and preview" }) as HTMLButtonElement).disabled).toBe(false);
});
