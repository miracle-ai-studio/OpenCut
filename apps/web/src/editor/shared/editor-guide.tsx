import { useEffect, useState } from "react";
import { ArrowRight, X } from "lucide-react";
import { preferences, t, useLocale } from "./locale";
import { Button, Modal } from "./ui";
import {
  readWelcomePreferences,
  saveWelcomePreferences,
  unseenUpstreamRelease,
} from "./view-preferences";
import { upstreamRelease } from "../../shared/upstream-release.generated";
export function EditorGuide({
  forced,
  onClose,
}: {
  forced: boolean;
  onClose: () => void;
}) {
  const { language } = useLocale(),
    [completed, setCompleted] = useState(
      () => readWelcomePreferences().completed,
    ),
    [step, setStep] = useState(0),
    [notice, setNotice] = useState(false);
  const open = forced || !completed;
  useEffect(() => {
    if (
      open ||
      !unseenUpstreamRelease(
        upstreamRelease.version,
        readWelcomePreferences().lastSeenRelease,
      )
    )
      return;
    saveWelcomePreferences({ lastSeenRelease: upstreamRelease.version });
    setNotice(true);
  }, [open]);
  useEffect(() => {
    if (open) setStep(0);
  }, [open]);
  const close = () => {
    saveWelcomePreferences({ completed: true });
    setCompleted(true);
    onClose();
  };
  const titles = [
    "Welcome to OpenCut",
    "Editing on your computer",
    "Made with the community",
  ];
  return (
    <>
      {open && (
        <Modal title={t(titles[step])} onClose={close} className="editor-guide">
          <div className="guide-progress" aria-label={t("Guide progress")}>
            <span>{step + 1} / 3</span>
            <select
              aria-label={t("Guide language")}
              value={language}
              onChange={(event) =>
                preferences({
                  language: event.target.value === "zh" ? "zh" : "en",
                })
              }
            >
              <option value="en">English</option>
              <option value="zh">中文</option>
            </select>
          </div>
          <div className="guide-body">
            {step === 0 ? (
              <>
                <p>{t("A free, open source editor for your stories.")}</p>
                <p>
                  {t(
                    "Import your video, audio or images, then place them on the timeline.",
                  )}
                </p>
              </>
            ) : step === 1 ? (
              <>
                <p>
                  {t(
                    "Your projects and imported media stay on this computer. Changes are saved automatically.",
                  )}
                </p>
                <p>
                  {t(
                    "Use the timeline to arrange and trim clips. Select an item to edit its properties, then preview and export your film.",
                  )}
                </p>
                <p>
                  {t("Discover the community’s plans for OpenCut.")}{" "}
                  <a href="/roadmap" target="_blank" rel="noopener noreferrer">
                    {t("Original roadmap")} ↗
                  </a>
                </p>
              </>
            ) : (
              <>
                <p>
                  {t("OpenCut grows through contributions from its community.")}
                </p>
                <p>
                  <a
                    href="https://discord.com/invite/Mu3acKZvCp"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {t("Join the community")} ↗
                  </a>
                </p>
                <p>
                  <a
                    href="/changelog"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {t("Read the original release notes")} ↗
                  </a>
                </p>
              </>
            )}
          </div>
          <div className="guide-buttons">
            {step > 0 && (
              <Button onClick={() => setStep((value) => value - 1)}>
                {t("Back")}
              </Button>
            )}
            <Button
              className="primary"
              onClick={() =>
                step < 2 ? setStep((value) => value + 1) : close()
              }
            >
              {t(step < 2 ? "Next" : "Finish")}
              <ArrowRight size={15} />
            </Button>
          </div>
        </Modal>
      )}
      {!open && notice && (
        <aside
          className="upstream-notice"
          aria-label={t("Original OpenCut update")}
        >
          <div>
            <strong>{t("Original OpenCut update")}</strong>
            <Button
              aria-label={t("Dismiss update")}
              icon={<X size={14} />}
              onClick={() => setNotice(false)}
            />
          </div>
          <span className="muted">
            {upstreamRelease.version} · {upstreamRelease.date}
          </span>
          <p>{upstreamRelease.title}</p>
          <a
            href={`/changelog/${upstreamRelease.version}`}
            target="_blank"
            rel="noopener noreferrer"
            onClick={() => setNotice(false)}
          >
            {t("Read release notes")} →
          </a>
        </aside>
      )}
    </>
  );
}
