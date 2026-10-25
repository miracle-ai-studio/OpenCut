// @vitest-environment jsdom
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { EditorApi } from "../../api";
import type { TextFont } from "./types";
import { synthesisFixture } from "./font-synthesis-fixture";
import { useSynthesisFonts } from "./font-synthesis-hook";

afterEach(cleanup);

it("does not starve pending Canvas proof on equal snapshot polls, while resource and connection changes retire it", async () => {
  const f = synthesisFixture();
  const pending: { signal: AbortSignal; resolve(value: unknown): void }[] = [];
  const api = {
    connectionIdentity: "one",
    sdk: {
      call: vi.fn((_name, _request, options) => new Promise((resolve) => {
        pending.push({ signal: options.signal, resolve });
      })),
    },
  };
  function Surface({ fonts }: { fonts: TextFont[] }) {
    const result = useSynthesisFonts(api as unknown as EditorApi, {
      projectId: "p", revision: "r",
    }, fonts);
    return <output>{result.loading ? "loading" : result.error ? "refused" : result.fonts[0].requested_style}</output>;
  }
  const proof = {
     project_id: "p", revision: "r",
    font_id: f.font.id, source: f.source,
  };
  const view = render(<Surface fonts={[f.font]} />);
  for (let i = 0; i < 3; i++) {
    view.rerender(<Surface fonts={structuredClone([f.font])} />);
  }
  expect(api.sdk.call).toHaveBeenCalledTimes(1);
  expect(pending[0].signal.aborted).toBe(false);
  await act(async () => pending[0].resolve(proof));
  expect(screen.getByText("italic")).toBeTruthy();
  view.rerender(<Surface fonts={structuredClone([f.font])} />);
  expect(screen.getByText("italic")).toBeTruthy();
  expect(api.sdk.call).toHaveBeenCalledTimes(1);

  const altered = structuredClone(f.font);
  altered.content.content_hash = "f".repeat(64);
  view.rerender(<Surface fonts={[altered]} />);
  expect(screen.getByText("loading")).toBeTruthy();
  expect(api.sdk.call).toHaveBeenCalledTimes(2);
  expect(pending[0].signal.aborted).toBe(true);
  await act(async () => pending[1].resolve(proof));
  expect(screen.getByText("refused")).toBeTruthy();

  api.connectionIdentity = "two";
  view.rerender(<Surface fonts={[f.font]} />);
  expect(screen.getByText("loading")).toBeTruthy();
  expect(api.sdk.call).toHaveBeenCalledTimes(3);
  expect(pending[1].signal.aborted).toBe(true);
  await act(async () => pending[2].resolve(proof));
  expect(screen.getByText("italic")).toBeTruthy();
});
