import fs from "node:fs/promises";
import path from "node:path";
import { expect } from "@playwright/test";

// The adapter is served only inside this acceptance page. This verifies actual
// browser DSP against independent waveforms; it is not a production Rust bridge.
export async function verifyClassicAudio(browser, endpoint, _directory, check) {
  const root = path.resolve(import.meta.dirname, "../../../packages/classic-audio-renderer");
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", e => errors.push(e.message));
  try {
    await page.route("**/__audio_acceptance/**", async route => {
      const relative = new URL(route.request().url()).pathname.slice("/__audio_acceptance/".length);
      const file = path.resolve(root, relative);
      if (!file.startsWith(root + path.sep) || !file.endsWith(".mjs") && !file.endsWith(".js")) return route.abort();
      await route.fulfill({ contentType: "text/javascript", body: await fs.readFile(file) });
    });
    await page.exposeFunction("terminateAudioAcceptanceRealm", () => page.close());
    await page.goto(`${endpoint}/editor`);
    const result = await page.evaluate(async engineIdentity => {
      const { runBrowserAudioAcceptance } = await import("/__audio_acceptance/tests/browser-acceptance.mjs");
      return runBrowserAudioAcceptance({ engineIdentity, abortRendering: () => globalThis.terminateAudioAcceptanceRealm() });
    }, `Chromium ${await browser.version()} · ${process.platform}/${process.arch}`);
    expect(errors).toEqual([]);
    check(`actual Chromium decoded WAV: linear 2× ${result.linearFrequencyHz.toFixed(3)} Hz; pinned SoundTouch preserves ${result.preservedFrequencyHz.toFixed(3)} Hz`);
    check("independent mono PCM reaches both channels exactly; whole-scene peak activates WebAudio mastering before range slicing");
  } finally { await page.close(); }
}
