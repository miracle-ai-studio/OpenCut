// Internal process infrastructure. Consumers supply fixed domain callbacks;
// audio wire and DSP remain in the audio entry point.
export { BrowserRealm } from "./browser.mjs";
export { runHost } from "./runner.mjs";
export { QuotaMonitor } from "./limits.mjs";
export { validateEnvelope, PacketReader, PacketWriter, CallbackBroker } from "./protocol.mjs";
export { moduleRegistry, exactDependency, playwrightRuntime, moduleResponse, ORIGIN, PACKAGE_ROOT } from "./modules.mjs";
