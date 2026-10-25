// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { LocalSessionButton } from "./local-session";
import { preferences } from "../shared/locale";
const context = vi.hoisted(() => ({ value: {} as unknown }));
vi.mock("./context", () => ({ useWorkspace: () => context.value, useEditingWorkspace: () => context.value }));
beforeEach(() => {
  preferences({ language: "en" });
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true;
  };
  HTMLDialogElement.prototype.close = function () {
    this.open = false;
  };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
function setup(active = true, token = "") {
  const controller = {
    canStartLocalSession: !token,
    start: vi.fn().mockResolvedValue(true),
    logoutLocalSession: vi.fn().mockResolvedValue(undefined),
  };
  context.value = {
    controller,
    api: {
      sdk: { baseUrl: location.origin },
      audioPreviewConnection: { token },
    },
    busy: false,
    connection: "connected",
  };
  const fetcher = vi.fn().mockResolvedValue({
    ok: true,
    json: async () => ({
      schema_version: 1,
      active,
      expires_in_seconds: active ? 200 : 0,
    }),
  });
  vi.stubGlobal("fetch", fetcher);
  render(<LocalSessionButton />);
  return { controller, fetcher };
}
it("offers a real local restart when the HTTP grant has expired", async () => {
  const { controller, fetcher } = setup(false);
  fireEvent.click(screen.getByRole("button", { name: "Local session" }));
  await screen.findByText(
    "The browser has no active local session. Start one to reconnect.",
  );
  fireEvent.click(screen.getByRole("button", { name: "Start local session" }));
  await waitFor(() => expect(controller.start).toHaveBeenCalledTimes(1));
  expect(controller.logoutLocalSession).not.toHaveBeenCalled();
  expect(fetcher).toHaveBeenCalledWith(
    "/app/session",
    expect.objectContaining({ credentials: "same-origin" }),
  );
});
it("keeps the logout retry visible until the service confirms revocation", async () => {
  const { controller } = setup();
  controller.logoutLocalSession.mockRejectedValueOnce(new Error("network"));
  fireEvent.click(screen.getByRole("button", { name: "Local session" }));
  fireEvent.click(screen.getByRole("button", { name: "End local session" }));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "could not be ended",
  );
  fireEvent.click(screen.getByRole("button", { name: "End local session" }));
  await waitFor(() =>
    expect(controller.logoutLocalSession).toHaveBeenCalledTimes(2),
  );
});
it("does not present local cookie controls for an explicit bearer connection", () => {
  const { fetcher } = setup(true, "private-token");
  expect(screen.queryByRole("button", { name: "Local session" })).toBeNull();
  expect(fetcher).not.toHaveBeenCalled();
});
it("keeps a failed start open with a connection retry instead of claiming logout failed", async () => {
  const { controller } = setup(false);
  controller.start.mockResolvedValueOnce(false);
  fireEvent.click(screen.getByRole("button", { name: "Local session" }));
  await screen.findByRole("button", { name: "Start local session" });
  fireEvent.click(screen.getByRole("button", { name: "Start local session" }));
  expect((await screen.findByRole("alert")).textContent).toContain("could not be started");
  expect(screen.getByRole("dialog")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Start local session" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(controller.start).toHaveBeenCalledTimes(2);
  expect(controller.logoutLocalSession).not.toHaveBeenCalled();
});
