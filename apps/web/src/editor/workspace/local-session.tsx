import { useEffect, useState } from "react";
import { useEditingWorkspace as useWorkspace } from "./context";
import { Button, Modal } from "../shared/ui";
import { useLocale } from "../shared/locale";

/** Local HTTP cookie lifecycle only; bearer credentials keep their own controls. */
export function LocalSessionButton() {
  const { controller, api, connection, busy } = useWorkspace(),
    { language } = useLocale();
  const t = (en: string, zh: string) => (language === "zh" ? zh : en);
  const [open, setOpen] = useState(false),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const [active, setActive] = useState<boolean | null>(null);
  useEffect(() => {
    if (!open) return;
    const abort = new AbortController();
    setActive(null);
    void fetch("/app/session", {
      credentials: "same-origin",
      signal: abort.signal,
    })
      .then(async (response) => {
        if (!response.ok) return;
        const value: unknown = await response.json();
        if (
          !abort.signal.aborted &&
          value &&
          typeof value === "object" &&
          "active" in value &&
          typeof value.active === "boolean"
        )
          setActive(value.active);
      })
      .catch(() => {});
    return () => abort.abort();
  }, [open]);
  const local = controller.canStartLocalSession && (
    !api ||
    (!api.audioPreviewConnection.token &&
      new URL(api.sdk.baseUrl).origin === location.origin));
  if (!local) return null;
  const run = async (end: boolean) => {
    setPending(true);
    setError(null);
    try {
      if (end) await controller.logoutLocalSession();
      else if (!await controller.start()) {
        setError(t(
          "The local session could not be started. Check the service, then retry connecting.",
          "本机会话未能启动，请检查服务后重试连接。",
        ));
        return;
      }
      setOpen(false);
    } catch {
      setError(
        t(
          end ? "The local session could not be ended. Retry ending it." : "The local session could not be started. Check the service, then retry connecting.",
          end ? "本机会话未能确认退出，请重试退出。" : "本机会话未能启动，请检查服务后重试连接。",
        ),
      );
    } finally {
      setPending(false);
    }
  };
  return (
    <>
      <Button
        disabled={busy || pending || connection === "connecting"}
        onClick={() => setOpen(true)}
      >
        {t("Local session", "本机会话")}
      </Button>
      {open && (
        <Modal
          title={t("Local browser session", "本地浏览器会话")}
          onClose={() => {
            if (!pending) setOpen(false);
          }}
        >
          <div className="padded">
            <p>
              {t(
                "This local session expires after eight hours. Ending it revokes this browser's cookie. Reopening the app starts a new local session.",
                "本机会话在八小时后过期。退出会撤销当前浏览器的 Cookie；重新打开应用会启动新的本机会话。",
              )}
            </p>
            <p>
              {t(
                "Cloud accounts are not used. Project files and accepted jobs remain on the local service.",
                "此应用不使用云账户。工程文件和已接收任务保留在本地服务中。",
              )}
            </p>
            {active === false && (
              <p role="status">
                {t(
                  "The browser has no active local session. Start one to reconnect.",
                  "当前浏览器没有有效的本机会话，请启动会话以重新连接。",
                )}
              </p>
            )}
            {error && <p role="alert">{error}</p>}
            {(api || active || error) && (
              <Button disabled={pending || busy} onClick={() => void run(true)}>
                {t("End local session", "退出本机会话")}
              </Button>
            )}
            {(!api || active === false) && (
              <Button
                disabled={pending || busy}
                onClick={() => void run(false)}
              >
                {t("Start local session", "启动本机会话")}
              </Button>
            )}
          </div>
        </Modal>
      )}
    </>
  );
}
