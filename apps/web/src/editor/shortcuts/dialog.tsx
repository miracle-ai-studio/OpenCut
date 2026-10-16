import { useState } from "react";
import { Button, Modal } from "../shared/ui";
import { ACTIONS, actionDefinition, type ShortcutActionId } from "./catalog";
import {
  bindingConflict,
  chordFromEvent,
  displayChord,
  reservedReason,
} from "./bindings";
import { useShortcuts } from "./provider";
import { useShortcutText } from "./messages";
import "./shortcuts.css";

export function ShortcutsButton() {
  const shortcuts = useShortcuts(),
    st = useShortcutText();
  return (
    <Button
      onClick={shortcuts.openHelp}
      title={shortcuts.label("show-shortcuts")}
    >
      {st("Shortcuts and settings")}
    </Button>
  );
}

export function ShortcutSettingsDialog({ onClose }: { onClose(): void }) {
  const shortcuts = useShortcuts(),
    st = useShortcutText();
  const [query, setQuery] = useState("");
  const [recording, setRecording] = useState<{
    action: ShortcutActionId;
    replace?: string;
  } | null>(null);
  const [candidate, setCandidate] = useState<string | null>(null),
    [error, setError] = useState<string | null>(null);
  const [resetting, setResetting] = useState(false),
    [config, setConfig] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const describeError = (cause: unknown) => {
    const message = cause instanceof Error ? cause.message : String(cause);
    if (message.startsWith("Shortcut conflict: "))
      return st("Shortcut conflict: {action}", {
        action: st(message.slice("Shortcut conflict: ".length)),
      });
    return st(message);
  };
  const run = (action: () => void) => {
    setError(null);
    setNotice(null);
    try {
      action();
    } catch (cause) {
      setError(describeError(cause));
    }
  };
  const record = (action: ShortcutActionId, replace?: string) => {
    setRecording({ action, replace });
    setCandidate(null);
    setError(null);
    setNotice(null);
  };
  const conflict =
    candidate && recording
      ? bindingConflict(shortcuts.bindings, recording.action, candidate)
      : null;
  const reserved =
    candidate && recording ? reservedReason(candidate, recording.action) : null;
  const entries = ACTIONS.filter((action) =>
    `${st(action.label)} ${action.label} ${st(action.category)} ${shortcuts.label(action.id)}`
      .toLowerCase()
      .includes(query.toLowerCase().trim()),
  );
  return (
    <Modal
      title={st("Keyboard shortcuts")}
      onClose={onClose}
      className="shortcuts-dialog"
    >
      <div className="shortcuts-body">
        <p>
          {st(
            "Keys use physical keyboard positions, as in Classic. Command on macOS becomes Control on Windows and Linux.",
          )}
        </p>
        <p className="muted">
          {st(
            "Text fields, IME composition, dialogs and focused controls keep their own keys. Timeline and asset actions require focus in that panel.",
          )}
        </p>
        <p className="muted">
          {st(
            "Hold navigation keys to repeat. Editing, playback toggles and history actions run once per press. Existing review and recovery steps still apply.",
          )}
        </p>
        <p className="muted">
          {st(
            "Escape also cancels local editing gestures. Some operating system shortcuts cannot be delivered to the browser.",
          )}
        </p>
        {shortcuts.notice && <p role="status">{st(shortcuts.notice)}</p>}
        {error && <p role="alert">{error}</p>}
        {notice && <p role="status">{st(notice)}</p>}
        <div className="toolbar">
          <input
            type="search"
            aria-label={st("Search shortcuts")}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
          <Button onClick={() => setResetting(true)}>
            {st("Reset all shortcuts")}
          </Button>
        </div>
        {resetting && (
          <section
            className="shortcut-confirm"
            aria-label={st("Restore default shortcuts?")}
          >
            <strong>{st("Restore default shortcuts?")}</strong>
            <p>
              {st(
                "This replaces every custom binding. Project contents and undo history stay unchanged.",
              )}
            </p>
            <Button
              onClick={() =>
                run(() => {
                  shortcuts.reset();
                  setResetting(false);
                  setRecording(null);
                })
              }
            >
              {st("Restore defaults")}
            </Button>
            <Button onClick={() => setResetting(false)}>{st("Cancel")}</Button>
          </section>
        )}
        {recording && (
          <section
            className="shortcut-recorder"
            aria-label={st("Record a shortcut")}
          >
            <strong>{st(actionDefinition(recording.action).label)}</strong>
            <p>
              {st(
                "Press a key combination. Escape cancels recording; Tab moves focus.",
              )}
            </p>
            <input
              key={`${recording.action}:${recording.replace ?? "new"}`}
              autoFocus
              readOnly
              aria-label={st("Record a shortcut")}
              value={
                candidate ? displayChord(candidate, shortcuts.platform) : ""
              }
              onKeyDown={(event) => {
                if (
                  event.nativeEvent.isComposing ||
                  event.nativeEvent.keyCode === 229 ||
                  event.repeat
                )
                  return;
                if (event.key === "Tab") return;
                event.preventDefault();
                event.stopPropagation();
                if (event.key === "Escape") {
                  setRecording(null);
                  setCandidate(null);
                  return;
                }
                const chord = chordFromEvent(
                  event.nativeEvent,
                  shortcuts.platform,
                );
                if (chord) {
                  setCandidate(chord);
                  setError(null);
                }
              }}
            />
            {conflict && (
              <p role="alert">
                {st(
                  "Shortcut conflict with {action}. Clear or change that action first.",
                  { action: st(actionDefinition(conflict).label) },
                )}
              </p>
            )}
            {reserved && <p role="alert">{st(reserved)}</p>}
            <Button
              disabled={!candidate || !!conflict || !!reserved}
              onClick={() =>
                run(() => {
                  shortcuts.setBinding(
                    recording.action,
                    candidate!,
                    recording.replace,
                  );
                  setRecording(null);
                  setCandidate(null);
                })
              }
            >
              {st("Save shortcut")}
            </Button>
            <Button
              onClick={() => {
                setRecording(null);
                setCandidate(null);
              }}
            >
              {st("Cancel")}
            </Button>
          </section>
        )}
        <table className="shortcut-table">
          <thead>
            <tr>
              <th>{st("Action")}</th>
              <th>{st("Keys")}</th>
              <th>{st("Scope")}</th>
              <th>{st("Editing")}</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((action) => {
              const available = shortcuts.availability(action.id),
                keys = shortcuts.bindings[action.id];
              return (
                <tr key={action.id} data-shortcut-action={action.id}>
                  <td>
                    <strong>{st(action.label)}</strong>
                    <small>{st(action.category)}</small>
                    <small>
                      {available.enabled
                        ? available.checked === undefined
                          ? st("Available")
                          : st(available.checked ? "Enabled" : "Disabled")
                        : st(available.reason!)}
                    </small>
                  </td>
                  <td>
                    {keys.length ? (
                      keys.map((key) => (
                        <span className="shortcut-binding" key={key}>
                          <Button
                            aria-label={`${st(action.label)}: ${st("Change {key}", { key: displayChord(key, shortcuts.platform) })}`}
                            onClick={() => record(action.id, key)}
                          >
                            <kbd>{displayChord(key, shortcuts.platform)}</kbd>
                          </Button>
                          <Button
                            aria-label={`${st(action.label)}: ${st("Clear {key}", { key: displayChord(key, shortcuts.platform) })}`}
                            onClick={() =>
                              run(() => shortcuts.clearBinding(action.id, key))
                            }
                          >
                            ×
                          </Button>
                        </span>
                      ))
                    ) : (
                      <span className="muted">{st("Unassigned")}</span>
                    )}
                  </td>
                  <td>
                    {st(
                      action.scope === "timeline"
                        ? "Timeline"
                        : action.scope === "assets"
                          ? "Assets"
                          : action.scope === "canvas"
                            ? "Preview canvas"
                            : "Workspace",
                    )}
                  </td>
                  <td>
                    <div className="toolbar">
                      <Button
                        disabled={keys.length >= 4}
                        aria-label={`${st(action.label)}: ${st("Add shortcut")}`}
                        onClick={() => record(action.id)}
                      >
                        {st("Add shortcut")}
                      </Button>
                      <Button
                        aria-label={`${st(action.label)}: ${st("Reset action")}`}
                        onClick={() => run(() => shortcuts.reset(action.id))}
                      >
                        {st("Reset action")}
                      </Button>
                      <Button
                        disabled={
                          !available.enabled ||
                          !!action.hold ||
                          action.id === "show-shortcuts"
                        }
                        title={
                          available.reason ? st(available.reason) : undefined
                        }
                        aria-label={`${st("Run")}: ${st(action.label)}`}
                        onClick={() => {
                          void shortcuts
                            .invoke(action.id)
                            .catch((cause) => setError(describeError(cause)));
                          onClose();
                        }}
                      >
                        {st("Run")}
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {!entries.length && <p>{st("No matching shortcuts.")}</p>}
        <details className="shortcut-transfer">
          <summary>{st("Import / export shortcut settings")}</summary>
          <textarea
            aria-label={st("Shortcut configuration JSON")}
            rows={8}
            value={config}
            maxLength={48 * 1024}
            spellCheck={false}
            onChange={(event) => setConfig(event.target.value)}
          />
          <div className="toolbar">
            <Button
              onClick={() =>
                run(() => {
                  shortcuts.importConfig(config);
                  setNotice("Configuration imported.");
                })
              }
            >
              {st("Import configuration")}
            </Button>
            <Button
              onClick={() => {
                setConfig(shortcuts.exportConfig());
                setError(null);
              }}
            >
              {st("Show current configuration")}
            </Button>
            <Button
              onClick={() => {
                const current = shortcuts.exportConfig();
                setConfig(current);
                if (!navigator.clipboard?.writeText) {
                  setNotice(
                    "Clipboard access is unavailable. Select and copy the configuration text.",
                  );
                  return;
                }
                void navigator.clipboard.writeText(current).then(
                  () => setNotice("Configuration copied."),
                  () =>
                    setNotice(
                      "Clipboard access is unavailable. Select and copy the configuration text.",
                    ),
                );
              }}
            >
              {st("Copy configuration")}
            </Button>
          </div>
        </details>
      </div>
    </Modal>
  );
}
