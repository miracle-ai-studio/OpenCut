/** Shared control-plane transport for SDK and editor adapters. No retries. */
export interface Envelope<T> {
  api_version: "1";
  request_id: string;
  data: T;
  error: null;
  warnings: string[];
}

/** The server rejected the request; details and request ID are preserved. */
export class OpenCutApiError extends Error {
  readonly name = "OpenCutApiError";
  constructor(
    readonly code: string,
    message: string,
    readonly details: unknown,
    readonly status: number,
    readonly requestId: string,
    readonly warnings: readonly string[],
  ) { super(message); }
}

/** A transport failure does not establish whether a mutation committed. */
export class OpenCutTransportError extends Error {
  readonly name = "OpenCutTransportError";
  readonly outcome = "unknown";
  constructor(cause: unknown) {
    super("OpenCut request could not be completed; inspect state or retry the identical idempotent request.", { cause });
  }
}

export class OpenCutProtocolError extends Error {
  readonly name = "OpenCutProtocolError";
  readonly outcome = "unknown";
  constructor(message: string, readonly status: number) { super(message); }
}

function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
function auditWarnings(response: Response, warnings: string[]): string[] {
  const warning = "DENIAL_AUDIT_UNAVAILABLE";
  return response.headers.get("x-opencut-audit-warning") === warning && !warnings.includes(warning)
    ? [...warnings, warning] : warnings;
}

export async function requestEnvelope<T>(fetcher: typeof globalThis.fetch, baseUrl: string,
  token: string, path: string, init: RequestInit = {}): Promise<Envelope<T>> {
  const url = new URL(baseUrl);
  if (!["http:","https:"].includes(url.protocol) || url.username || url.password || url.search || url.hash)
    throw new TypeError("baseUrl must be an HTTP(S) URL without credentials, query or fragment.");
  if (!path.startsWith("/v1/") || path.includes("\\")) throw new TypeError("Expected an OpenCut v1 resource path.");
  init.signal?.throwIfAborted();
  const headers = new Headers(init.headers);
  headers.set("Authorization", `Bearer ${token}`);
  headers.set("Accept", "application/json");
  if (!headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  let response: Response;
  let value: unknown;
  try {
    response = await fetcher(`${url.href.replace(/\/$/, "")}${path}`, { ...init, redirect: "error", headers: Object.fromEntries(headers.entries()) });
    const raw = await response.text();
    try { value = JSON.parse(raw); }
    catch { throw new OpenCutProtocolError("OpenCut returned a non-JSON response.", response.status); }
  } catch (cause) {
    if (init.signal?.aborted) throw init.signal.reason;
    if (cause instanceof OpenCutProtocolError) throw cause;
    throw new OpenCutTransportError(cause);
  }
  if (!object(value) || value.api_version !== "1" || typeof value.request_id !== "string" ||
    !Array.isArray(value.warnings) || !value.warnings.every(w => typeof w === "string"))
    throw new OpenCutProtocolError("Invalid or incompatible OpenCut response envelope.", response.status);
  if (object(value.error) && typeof value.error.code === "string" && typeof value.error.message === "string")
    throw new OpenCutApiError(value.error.code,value.error.message,value.error.details,response.status,value.request_id,auditWarnings(response,value.warnings));
  if (!response.ok || value.error !== null || value.data === null || value.data === undefined)
    throw new OpenCutProtocolError("Inconsistent OpenCut response status or payload.",response.status);
  return value as unknown as Envelope<T>;
}
