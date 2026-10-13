import { chromium } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { verifyTimelineSelection } from "./verify-timeline-selection.mjs";
import { verifyMediaFps, verifyV4MediaFps } from "./verify-media-fps.mjs";
import { verifyProjectCovers } from "./verify-project-covers.mjs";
import { verifyFeedback } from "./verify-feedback.mjs";
import { verifyV4Video } from "./verify-v4-video.mjs";
import { verifyV4CanvasText } from "./verify-v4-canvas-text.mjs";
import { verifyV4FontBrowser } from "./verify-v4-font-browser.mjs";
import { verifyV4Captions } from "./verify-v4-captions.mjs";
import { verifyV4Asr } from "./verify-v4-asr.mjs";
import { verifyV4CaptionVideo } from "./verify-v4-caption-video.mjs";
import { verifyV4TextTimeline } from "./verify-v4-text-timeline.mjs";
import { verifyV4TextTransform } from "./verify-v4-text-transform.mjs";
import { verifyV4BookmarkLifecycle } from "./verify-v4-bookmark-lifecycle.mjs";
import { verifyV4Text } from "./verify-v4-text.mjs";
import { verifyV4Editor } from "./verify-v4-editor.mjs";
import { verifyClassicAudio } from "./verify-classic-audio.mjs";
import { verifyV4Performance } from "./verify-v4-timeline.mjs";
import { verifySourceArt } from "./verify-source-art.mjs";
import { verifyEditRecovery } from "./verify-edit-recovery.mjs";
import { verifyV4FileImport } from "./verify-v4-file-import.mjs";

import {
  verifyV4ScenePreview,
  verifyV4ScenePreviewBoundaries,
} from "./verify-v4-scene-preview.mjs";

import { verifyV4ScenePreviewPerformance } from "./verify-v4-scene-preview-performance.mjs";

import { verifyV4ScenePreviewContinuity } from "./verify-v4-scene-preview-continuity.mjs";
import { verifyV4ScenePreviewIdentity } from "./verify-v4-scene-preview-identity.mjs";
import { verifyV4ScenePreviewGain } from "./verify-v4-scene-preview-gain.mjs";
import { verifyV4ScenePreviewSeek } from "./verify-v4-scene-preview-seek.mjs";
import { verifyV4ScenePreviewRetirement } from "./verify-v4-scene-preview-retirement.mjs";
import { verifyV4ScenePreviewPcm } from "./verify-v4-scene-preview-pcm.mjs";
import { verifyV4ScenePreviewAcoustics } from "./verify-v4-scene-preview-acoustics.mjs";

const suites = new Map([
  ["scene-preview", verifyV4ScenePreview],
  ["scene-preview-acoustics", verifyV4ScenePreviewAcoustics],
  ["scene-preview-acoustics-hot", (...args) => verifyV4ScenePreviewAcoustics(...args, {hotOnly:true})],
  ["scene-preview-acoustics-gap", (...args) => verifyV4ScenePreviewAcoustics(...args, {hotOnly:true,controlledGap:true})],
  ["scene-preview-continuity", verifyV4ScenePreviewContinuity],
  ["scene-preview-retirement", verifyV4ScenePreviewRetirement],
  ["scene-preview-identity", verifyV4ScenePreviewIdentity],
  ["scene-preview-revocation", (...args) => verifyV4ScenePreviewIdentity(...args, {revocationOnly: true})],
  ["scene-preview-gain", verifyV4ScenePreviewGain],
  ["scene-preview-gain-paused", (...args) => verifyV4ScenePreviewGain(...args, {pausedOnly: true})],
  ["scene-preview-seek", verifyV4ScenePreviewSeek],
  ["scene-preview-seek-pause", (...args) => verifyV4ScenePreviewSeek(...args, {pauseAfterSeek: true})],
  ["scene-preview-pcm", verifyV4ScenePreviewPcm],
  ["scene-preview-pcm-terminal", (...args) => verifyV4ScenePreviewPcm(...args, {terminalOnly: true})],
  ["scene-preview-performance", verifyV4ScenePreviewPerformance],
  ["scene-preview-hd", (...args) => verifyV4ScenePreview(...args, { width: 1280, height: 720 })],
  ["scene-preview-boundaries", verifyV4ScenePreviewBoundaries],
  ["timeline-selection", verifyTimelineSelection],
  ["media-fps", verifyMediaFps],
  ["v4-media-fps", verifyV4MediaFps],
  ["project-covers", verifyProjectCovers],
  ["feedback", verifyFeedback],
  ["v4-editor", verifyV4Editor],
  ["v4-text", verifyV4Text],
  ["v4-canvas-text", verifyV4CanvasText],
  ["v4-canvas-text-blur", (...args) => verifyV4CanvasText(...args, { blurOnly: true })],
  ["v4-bookmark-lifecycle", verifyV4BookmarkLifecycle],
  ["v4-font-browser", verifyV4FontBrowser],
  ["v4-font-browser-offline", (...args) => verifyV4FontBrowser(...args, { offlineOnly: true })],
  ["v4-captions", verifyV4Captions],
  ["v4-asr", verifyV4Asr],
  ["v4-caption-video", verifyV4CaptionVideo],
  ["v4-text-timeline", verifyV4TextTimeline],
  ["v4-text-timeline-resume", (...args) => verifyV4TextTimeline(...args, { resumeFromCheckpoint: true })],
  ["v4-text-transform", verifyV4TextTransform],
  ["v4-video", verifyV4Video],
  ["v4-text-layout", (...args) => verifyV4Text(...args, {layoutOnly:true})],
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
