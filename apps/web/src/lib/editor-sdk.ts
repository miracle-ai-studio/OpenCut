export {
  OpenCutClient,
  OpenCutApiError,
  OpenCutProtocolError,
  OpenCutTransportError,
  JobCompletionError,
  publicJobResult,
  requestEnvelope,
} from "../../../../packages/sdk-ts/src/index";
export type {
  AudioPreviewPcmHeaderV4,
  ToolInput,
  ToolOutput,
  ToolName,
  PublicJob,
  ArtifactReference,
  HostCapabilities,
  Envelope,
  ClientOptions,
} from "../../../../packages/sdk-ts/src/index";

import { OpenCutClient as Client } from "../../../../packages/sdk-ts/src/index";
import type { ClientOptions as Options } from "../../../../packages/sdk-ts/src/index";
export type EditorConnection = Pick<Options, "baseUrl" | "token">;
/** Shared cookie/bearer transport assembly for the UI and its private workers. */
export function createEditorClient(connection: EditorConnection) {
  return new Client({
    ...connection,
    fetch: connection.token
      ? undefined
      : async (input, init) => {
          const headers = new Headers(init?.headers);
          headers.delete("Authorization");
          return fetch(input, { ...init, headers, credentials: "same-origin" });
        },
  });
}
