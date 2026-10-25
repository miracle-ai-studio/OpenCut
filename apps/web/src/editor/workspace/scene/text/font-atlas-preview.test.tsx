// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { FontAtlasPreview } from "./font-atlas-preview";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
it("uses the Classic 40px mask coordinates and distinguishes the family specimen from face rendering", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValue({
      ok: true,
      json: async () => ({ fonts: { Lato: { x: 17, y: 80, w: 120, ch: 2 } } }),
    });
  vi.stubGlobal("fetch", fetcher);
  const { rerender } = render(<FontAtlasPreview family="Lato" />);
  const image = await screen.findByRole("img", {
    name: "Lato family specimen",
  });
  expect(image.style.height).toBe("40px");
  expect(image.style.maskPosition).toBe("-17px -80px");
  expect(image.style.maskImage).toContain("font-chunk-2.avif");
  expect(screen.getByText(/The selected weight, style and your text/)).toBeTruthy();
  expect(fetcher).toHaveBeenCalledWith("/assets/classic-font-atlas/font-atlas.json");
  rerender(<FontAtlasPreview family="Arial" />);
  expect(screen.queryByRole("img")).toBeNull();
  expect(screen.getByText("No Classic family specimen.").textContent).toBe(
    "No Classic family specimen.",
  );
});
