import { t, useLocale } from "./locale";
export function localSessionExpired(error: unknown): boolean {
  return /valid bearer token is required|local browser session expired or ended/i.test(error instanceof Error ? error.message : String(error));
}
/** Backend details stay available for support, outside the primary instruction. */
export function friendlyError(error: unknown, localSession = true): string {
  const raw = error instanceof Error ? error.message : String(error);
  if (raw === "Confirm or cancel the property draft before changing selection." ||
      raw === "Confirm or cancel the property draft before starting another edit.") return t(raw);
  if (raw === "Reconnect to the original service with valid credentials.") return t(raw);
  if (localSessionExpired(error)) return t(localSession
    ? "Your local session expired or the service restarted. Reconnect to continue. Saved edits remain available."
    : "Your connection credentials were rejected. Reconnect to the original service with valid credentials. Saved edits remain available.");
  if (/Preview optimization/i.test(raw)) return t("Preview optimization could not finish. Retry playback.");
  if (/Preview cache exceeds/i.test(raw)) return t("Preview cache exceeds its memory limit.");
  if (/browser.*audio output clock/i.test(raw)) return t("This browser cannot synchronize preview playback. Update your browser and try again.");
  if (/audio.*(?:output interval|sample interval)/i.test(raw)) return t("Audio playback lost its timing. Press Play to resume.");
  if (/audio.*(?:output clock|device clock|output position|output interval|sample interval)/i.test(raw)) return t("The audio device changed or stopped responding. Press Play to try again.");
  if (/crossfade.*one-to-one|Legacy crossfade|sufficient visible time/i.test(raw)) return t("Use two normal-speed clips with enough length. Shorten the transition or restore normal speed.");
  if (/location dialog|choosing.*location|selected package location|cannot open.*location/i.test(raw)) return t("Choose the location again. If a selection window is already open, finish or cancel it first.");
  if (/changed|conflict|stale|revision/i.test(raw)) return t("The project changed. Refresh it and try again.");
  if (/network|fetch|connect|timeout|timed out|unconfirmed/i.test(raw)) return t("The connection was interrupted. Check your connection, then retry the same action.");
  if (/permission|denied|unauthorized|forbidden/i.test(raw)) return t("OpenCut cannot access this file or location. Check its permissions and try again.");
  if (/not found|missing|unavailable/i.test(raw)) return t("The required file or feature is unavailable. Check your media and try again.");
  if (/exist|destination/i.test(raw)) return t("This save location cannot be used. Choose a new folder name and try again.");
  if (/range|positive|number|invalid|Enter /i.test(raw)) return t("Check the values you entered and try again.");
  return t("This action could not be completed. Your saved project is still available. Please try again.");
}
export function ErrorNotice({ error, localSession = true }: { error: unknown; localSession?: boolean }) {
  useLocale();
  return <div className="error-notice"><p role="alert">{friendlyError(error, localSession)}</p><details className="diagnostic-details"><summary>{t("Technical details")}</summary><pre>{error instanceof Error ? error.message : String(error)}</pre></details></div>;
}
