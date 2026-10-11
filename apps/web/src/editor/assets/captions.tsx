import { AutomaticCaptions } from "./automatic-captions";
import { Subtitles } from "./subtitles";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { formatTime } from "../model/time";
import { Panel, Empty, Button } from "../shared/ui";
import { t, useLocale } from "../shared/locale";
export function CaptionsPanel() {
  useLocale();
  const { controller, snapshot, historical, sequenceId } = useWorkspace();
  const sequence = activeSequence(historical ?? snapshot, sequenceId);
  return (
    <Panel title={t("Captions")}>
      <div className="padded scroll">
        <AutomaticCaptions />
        <Subtitles />
        {sequence?.caption_cues?.length ? (
          <div className="caption-list">
            {sequence.caption_cues.map((cue) => (
              <Button
                key={cue.id}
                onClick={() => {
                  controller.seek(cue.timeline_range.start);
                  controller.select({ kind: "caption", id: cue.id });
                }}
              >
                <span className="muted">
                  {formatTime(cue.timeline_range.start)}
                </span>
                <span>{cue.text}</span>
              </Button>
            ))}
          </div>
        ) : (
          <Empty title={t("Captions")}>{t("Import SRT / ASS")}</Empty>
        )}
      </div>
    </Panel>
  );
}
