import { chromium } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { verifyTimelineSelection } from "./verify-timeline-selection.mjs";
import { verifyMediaFps, verifyV4MediaFps } from "./verify-media-fps.mjs";
import { verifyProjectCovers } from "./verify-project-covers.mjs";
import { verifyFeedback } from "./verify-feedback.mjs";
import { verifyV4Editor } from "./verify-v4-editor.mjs";
import { verifyClassicAudio } from "./verify-classic-audio.mjs";
import { verifyV4Performance } from "./verify-v4-timeline.mjs";
import { verifySourceArt } from "./verify-source-art.mjs";
import { verifyEditRecovery } from "./verify-edit-recovery.mjs";
import { verifyV4FileImport } from "./verify-v4-file-import.mjs";

const suites = new Map([
  ["timeline-selection", verifyTimelineSelection],
  ["media-fps", verifyMediaFps],
  ["v4-media-fps", verifyV4MediaFps],
  ["project-covers", verifyProjectCovers],
  ["feedback", verifyFeedback],
  ["v4-editor", verifyV4Editor],
  ["classic-audio", verifyClassicAudio],
  ["v4-performance", verifyV4Performance],
  ["source-art", verifySourceArt],
  ["edit-recovery", verifyEditRecovery],
  ["v4-file-import", verifyV4FileImport],
  [
    "v4-file-import-boundaries",
    (...args) => verifyV4FileImport(...args, { boundaries: true }),
  ],
]);
const names = (process.env.OPENCUT_WEB_SUITE ?? "").split(",");
const verifications = names.map((name) => suites.get(name));
const directory = process.env.OPENCUT_WEB_WORKSPACE;
const endpoint = process.env.OPENCUT_WEB_URL;
if (verifications.some((verify) => !verify) || !directory || !endpoint)
  throw new Error(
    "Run scripts/verify-web.py --suite with an isolated workspace.",
  );

const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const report = {
  browser: await browser.version(),
  suites: names,
  checks: [],
  evidence: [],
};
try {
  for (const verify of verifications)
    await verify(browser, endpoint, directory, (name, evidence) => {
      report.checks.push(name);
      if (evidence) report.evidence.push({ check: name, ...evidence });
      console.log(name);
    });
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error);
  console.error(error);
  process.exitCode = 1;
} finally {
  try {
    await fs.writeFile(
      path.join(directory, "browser-report.json"),
      JSON.stringify(report, null, 2),
    );
  } finally {
    await browser.close();
  }
}
