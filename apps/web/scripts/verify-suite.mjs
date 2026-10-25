import { chromium } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { verifySceneMediaFps } from "./verify-media-fps.mjs";
import { verifyProjectCovers } from "./verify-project-covers.mjs";
import { verifyFeedback } from "./verify-feedback.mjs";
import { verifyVideo } from "./verify-video.mjs";
import { verifyCanvasText } from "./verify-canvas-text.mjs";
import { verifyFontBrowser } from "./verify-font-browser.mjs";
import { verifyCaptions } from "./verify-captions.mjs";
import { verifyAsr } from "./verify-asr.mjs";
import { verifyCaptionVideo } from "./verify-caption-video.mjs";
import { verifyTextTimeline } from "./verify-text-timeline.mjs";
import { verifyTextTransform } from "./verify-text-transform.mjs";
import { verifyTextStyle } from "./verify-text-style.mjs";
import { verifyEditableSubtitles } from "./verify-editable-subtitles.mjs";
import { verifyAsrEditable } from "./verify-asr-editable.mjs";
import { verifyDerivedFont } from "./verify-derived-font.mjs";
import { verifyAssImport } from "./verify-ass-import.mjs";
import { verifyMask } from "./verify-mask.mjs";
import { verifySceneStack } from "./verify-scene-stack.mjs";
import { verifyParallelEditing } from "./verify-parallel-editing.mjs";
import { verifyBlend } from "./verify-blend.mjs";
import { verifyGraphic } from "./verify-graphic.mjs";
import { verifyProjectPackage } from "./verify-package-current.mjs";
import { verifyPackageOutput } from "./verify-package-output.mjs";
import { verifyGraphicOutput } from "./verify-graphic-output.mjs";
import { verifyBookmarkLifecycle } from "./verify-bookmark-lifecycle.mjs";
import { verifyText } from "./verify-text.mjs";
import { verifyEditor } from "./verify-editor.mjs";
import { verifyClassicAudio } from "./verify-classic-audio.mjs";
import { verifyPerformance } from "./verify-timeline.mjs";
import { verifySourceArt } from "./verify-source-art.mjs";
import { verifyEditRecovery } from "./verify-edit-recovery.mjs";
import { verifyFileImport } from "./verify-file-import.mjs";
import { verifyBasicFlow } from "./verify-basic-flow.mjs";

import {
  verifyScenePreview,
  verifyScenePreviewBoundaries,
} from "./verify-scene-preview.mjs";

import { verifyScenePreviewPerformance } from "./verify-scene-preview-performance.mjs";

import { verifyScenePreviewContinuity } from "./verify-scene-preview-continuity.mjs";
import { verifyScenePreviewIdentity } from "./verify-scene-preview-identity.mjs";
import { verifyScenePreviewGain } from "./verify-scene-preview-gain.mjs";
import { verifyScenePreviewSeek } from "./verify-scene-preview-seek.mjs";
import { verifyScenePreviewRetirement } from "./verify-scene-preview-retirement.mjs";
import { verifyScenePreviewPcm } from "./verify-scene-preview-pcm.mjs";
import { verifyScenePreviewAcoustics } from "./verify-scene-preview-acoustics.mjs";

const suites = new Map([
  ["basic-flow", verifyBasicFlow],
  ["basic-snapshots", (...args) => verifyBasicFlow(...args, { snapshotsOnly: true })],
  ["full", verifyEditor],
  ["scene-preview", verifyScenePreview],
  ["scene-preview-acoustics", verifyScenePreviewAcoustics],
  ["scene-preview-acoustics-hot", (...args) => verifyScenePreviewAcoustics(...args, {hotOnly:true})],
  ["scene-preview-acoustics-gap", (...args) => verifyScenePreviewAcoustics(...args, {hotOnly:true,controlledGap:true})],
  ["scene-preview-continuity", verifyScenePreviewContinuity],
  ["scene-preview-retirement", verifyScenePreviewRetirement],
  ["scene-preview-identity", verifyScenePreviewIdentity],
  ["scene-preview-revocation", (...args) => verifyScenePreviewIdentity(...args, {revocationOnly: true})],
  ["scene-preview-gain", verifyScenePreviewGain],
  ["scene-preview-gain-paused", (...args) => verifyScenePreviewGain(...args, {pausedOnly: true})],
  ["scene-preview-seek", verifyScenePreviewSeek],
  ["scene-preview-seek-pause", (...args) => verifyScenePreviewSeek(...args, {pauseAfterSeek: true})],
  ["scene-preview-pcm", verifyScenePreviewPcm],
  ["scene-preview-pcm-terminal", (...args) => verifyScenePreviewPcm(...args, {terminalOnly: true})],
  ["scene-preview-performance", verifyScenePreviewPerformance],
  ["scene-preview-hd", (...args) => verifyScenePreview(...args, { width: 1280, height: 720 })],
  ["scene-preview-boundaries", verifyScenePreviewBoundaries],
  ["media-fps", verifySceneMediaFps],
  ["project-covers", verifyProjectCovers],
  ["feedback", verifyFeedback],
  ["editor", verifyEditor],
  ["text", verifyText],
  ["canvas-text", verifyCanvasText],
  ["canvas-text-blur", (...args) => verifyCanvasText(...args, { blurOnly: true })],
  ["bookmark-lifecycle", verifyBookmarkLifecycle],
  ["font-browser", verifyFontBrowser],
  ["font-browser-offline", (...args) => verifyFontBrowser(...args, { offlineOnly: true })],
  ["captions", verifyCaptions],
  ["asr", verifyAsr],
  ["caption-video", verifyCaptionVideo],
  ["text-timeline", verifyTextTimeline],
  ["text-timeline-resume", (...args) => verifyTextTimeline(...args, { resumeFromCheckpoint: true })],
  ["text-transform", verifyTextTransform],
  ["text-style", verifyTextStyle],
  ["editable-subtitles", verifyEditableSubtitles],
  ["asr-editable", verifyAsrEditable],
  ["ass-import", verifyAssImport],
  ["graphic", verifyGraphic],
  ["blend", verifyBlend],
  ["mask", verifyMask],
  ["scene-stack", verifySceneStack],
  ["parallel-editing", verifyParallelEditing],
  ["blend-outputs", (...args) => verifyBlend(...args, { outputsOnly: true })],
  ["graphic-output", verifyGraphicOutput],
  ["package", verifyProjectPackage],
  ["package-output", verifyPackageOutput],
  ["derived-font", verifyDerivedFont],
  ["derived-font-offline", (...args) => verifyDerivedFont(...args, { offlineOnly: true })],
  ["video", verifyVideo],
  ["text-layout", (...args) => verifyText(...args, {layoutOnly:true})],
  ["classic-audio", verifyClassicAudio],
  ["performance", verifyPerformance],
  ["source-art", verifySourceArt],
  ["edit-recovery", verifyEditRecovery],
  ["file-import", verifyFileImport],
  [
    "file-import-boundaries",
    (...args) => verifyFileImport(...args, { boundaries: true }),
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
