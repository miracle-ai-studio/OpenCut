import { Select } from "./controls";
import { useEffect, useState } from "react";
import { ArrowRight } from "lucide-react";
import { preferences, t, useLocale } from "./locale";
import { Button, Modal } from "./ui";
import {
  readWelcomePreferences,
  saveWelcomePreferences,
} from "./view-preferences";
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
    [step, setStep] = useState(0);
  const open = forced || !completed;
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
    language === "zh" ? "与 Agent 共同剪辑" : "Edit with an Agent",
  ];
  return (
    <>
      {open && (
        <Modal title={t(titles[step])} onClose={close} className="editor-guide">
          <div className="guide-progress" aria-label={t("Guide progress")}>
            <span>{step + 1} / 3</span>
            <Select
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
            </Select>
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
              </>
            ) : (
              <>
                <p>
                  {language === "zh"
                    ? "Agent 与你使用同一工程和编辑操作。你可以继续在编辑器中查看和调整结果。"
                    : "An Agent uses the same project and editing operations. You can review and adjust its work in the editor."}
                </p>
                <p>
                  {language === "zh"
                    ? "在历史记录中审阅修改。工程已被其他操作者更新时，请先同步并检查新修订，再提交原来的计划。"
                    : "Review changes in history. When another editor updates the project, sync and review the new revision before submitting an old plan."}
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
    </>
  );
}
