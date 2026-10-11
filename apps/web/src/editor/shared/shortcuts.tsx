import { t, useLocale } from "./locale";
import { Modal } from "./ui";
export function ShortcutsDialog({ onClose }: { onClose: () => void }) {
  useLocale();
  const apple =
    typeof navigator !== "undefined" && /Mac/.test(navigator.platform);
  const modifier = apple ? "⌘" : "Ctrl";
  return (
    <Modal title={t("Keyboard shortcuts")} onClose={onClose}>
      <div className="shortcut-list">
        {[
          ["Playback", "Space"],
          ["Split clip", "S"],
          ["Delete clip", "⌫"],
          ["Undo", `${modifier} Z`],
          ["Redo", `${modifier} ⇧ Z`],
          ["Copy", `${modifier} C`],
          ["Paste", `${modifier} V`],
          ["Duplicate", `${modifier} D`],
          ["Previous frame", "←"],
          ["Next frame", "→"],
        ].map(([name, key]) => (
          <div key={name}>
            <span>{t(name)}</span>
            <kbd>{key}</kbd>
          </div>
        ))}
      </div>
    </Modal>
  );
}
