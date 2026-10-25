import { Select } from "./shared/controls";
import { Group, Panel as ResizablePanel, Separator } from "react-resizable-panels";
import { useState } from "react";
import { useWorkspace } from "./workspace/context";
import { activeSequence } from "./model/types";
import { PreviewPanel } from "./preview/panel";
import { HistoryPanel } from "./history/panel";
import { Button, Field, Panel } from "./shared/ui";
import { t, useLocale } from "./shared/locale";

/** Historical data is viewed in place;the current editor owns all authoring. */
export function HistoricalProject() {
  useLocale();
  const { controller, snapshot, historical, sequenceId, busy } = useWorkspace();
  const [converting, setConverting] = useState(false);
  const view = historical ?? snapshot, sequence = activeSequence(view, sequenceId);
  if (!view) return null;
  const convert = async () => {
    setConverting(true);
    try { await controller.promoteProject(); } catch (error) { controller.report(error); }
    finally { setConverting(false); }
  };
  return <section className="workspace-layout" aria-label={t("Historical project")}>
    <div className="toolbar padded">
      <span>{t("Historical source is read-only. Convert it to edit in OpenCut.")}</span>
      <Button disabled={busy || converting || !!historical} onClick={() => void convert()}>{t(converting ? "Converting…" : "Convert for editing")}</Button>
      {historical && <Button onClick={() => controller.returnToHead()}>{t("Return to current")}</Button>}
    </div>
    <Group orientation="horizontal" className="stage-panels">
      <ResizablePanel id="historical-source" defaultSize="25%" minSize="220px">
        <Panel title={t("Source")}> <div className="padded">
          <Field label={t("Scene")}><Select value={sequence?.id ?? ""} onChange={event => controller.chooseSequence(event.target.value)}>
            {view.project.sequences.map(scene => <option key={scene.id} value={scene.id}>{scene.name}</option>)}
          </Select></Field>
          <p>{view.project.name}</p><p className="muted">{view.revision}</p>
          {sequence && <p>{sequence.width} × {sequence.height} · {sequence.fps.numerator / sequence.fps.denominator} fps</p>}
          <ul>{view.project.assets.map(asset => <li key={asset.id}>{asset.name} · {asset.kind}</li>)}</ul>
          <ul>{sequence?.tracks.map(track => <li key={track.id}>{track.name} · {track.clips.length}</li>)}</ul>
        </div></Panel>
      </ResizablePanel>
      <Separator className="resize-handle" />
      <ResizablePanel id="historical-preview" defaultSize="50%" minSize="30%"><PreviewPanel /></ResizablePanel>
      <Separator className="resize-handle" />
      <ResizablePanel id="historical-history" defaultSize="25%" minSize="220px"><HistoryPanel /></ResizablePanel>
    </Group>
  </section>;
}
