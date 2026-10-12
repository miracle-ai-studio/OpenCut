// Self-contained old-origin bookmark exporter. No native service/network calls,
// modern project authority, old database writes or media deletion are included.
import { captureClassic, discoverClassic, readClassicFile } from "./source";
import { createRecoveryBundle } from "./bundle";
import { fileDigest } from "./identity";
async function launch() {
  const existing = document.getElementById("opencut-classic-exporter");
  if (existing) {
    existing.dispatchEvent(new Event("cancel", { cancelable: true }));
    existing.remove();
  }
  const dialog = document.createElement("dialog");
  dialog.id = "opencut-classic-exporter";
  dialog.style.cssText =
    "width:560px;max-width:90vw;max-height:85vh;overflow:auto;padding:24px;border:1px solid #555;border-radius:12px;background:#202022;color:#eee;font:14px system-ui;";
  const title = document.createElement("h2");
  title.textContent = "Export Classic projects / 导出旧工程";
  const explanation = document.createElement("p");
  explanation.textContent = `Read-only export from ${location.origin}. Original projects and files stay here. / 只读导出，原工程与媒体保留在此站点。`;
  const status = document.createElement("p");
  status.setAttribute("role", "status");
  const select = document.createElement("select");
  select.setAttribute("aria-label", "Classic project");
  select.style.cssText = "width:100%;padding:8px;margin:12px 0;";
  const button = document.createElement("button");
  button.textContent = "Export recovery bundle / 导出恢复包";
  const close = document.createElement("button");
  close.textContent = "Close / 关闭";
  close.style.marginLeft = "12px";
  let abort: AbortController | undefined;
  close.onclick = () => {
    abort?.abort();
    dialog.close();
    dialog.remove();
  };
  dialog.oncancel = (event) => {
    event.preventDefault();
    close.click();
  };
  dialog.append(title, explanation, select, status, button, close);
  document.body.append(dialog);
  dialog.showModal();
  try {
    const sources = await discoverClassic();
    for (const source of sources) {
      const option = document.createElement("option");
      option.value = source.id;
      option.textContent = `${source.name} · v${source.version}`;
      option.disabled = !!source.error;
      select.append(option);
    }
    button.disabled = !sources.some((s) => !s.error);
    status.textContent = sources.length
      ? `${sources.length} old projects / 个旧工程`
      : "No Classic projects on this origin / 当前站点未发现旧工程";
  } catch (error) {
    status.textContent = String(error);
    button.disabled = true;
  }
  button.onclick = async () => {
    button.disabled = true;
    select.disabled = true;
    abort = new AbortController();
    try {
      const snapshot = await captureClassic(select.value, abort.signal),
        media = [];
      for (const [index, row] of snapshot.media.entries()) {
        status.textContent = `${index + 1}/${snapshot.media.length}: ${String(row.name ?? row.id)}`;
        const file = await readClassicFile(snapshot.sourceId, row);
        media.push({
          id: String(row.id),
          file,
          sha256: await fileDigest(file, abort.signal),
        });
      }
      const blob = await createRecoveryBundle(snapshot, media, abort.signal);
      const current = await captureClassic(snapshot.sourceId, abort.signal);
      if (JSON.stringify(current) !== JSON.stringify(snapshot))
        throw new Error(
          "Classic project changed during export; retry / 导出期间工程变化，请重试",
        );
      abort.signal.throwIfAborted();
      const link = document.createElement("a"),
        url = URL.createObjectURL(blob);
      link.href = url;
      link.download = "classic-project.occlassic";
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 60000);
      status.textContent =
        "Export complete. Import this bundle in OpenCut. / 导出完成，请在 OpenCut 中选择此恢复包。";
    } catch (error) {
      if (!abort.signal.aborted) status.textContent = String(error);
    } finally {
      button.disabled = false;
      select.disabled = false;
    }
  };
}
void launch();
