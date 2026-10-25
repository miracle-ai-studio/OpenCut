import { OpenCutApiError } from "../../../../lib/editor-sdk";
import { localSessionExpired } from "../../../shared/error-notice";

/** Presentation only. Keep the service receipt intact for diagnosis. */
export type SourceArtFailure = {
  message: string; code?: string; requestId?: string; status?: number;
  kind: "connection" | "authorization" | "budget" | "unavailable"; retryable: boolean;
};
export function sourceArtFailure(error: unknown): SourceArtFailure {
  const message = error instanceof Error ? error.message : "Source artwork unavailable.";
  const receipt = error instanceof OpenCutApiError
    ? { code: error.code, requestId: error.requestId, status: error.status } : {};
  const connection = receipt.status === 401 || localSessionExpired(error);
  const authorization = connection || receipt.status === 403 || receipt.code === "NOT_AUTHORIZED" || message === "NOT_AUTHORIZED";
  const budget = receipt.code === "SOURCE_ART_BUDGET_EXCEEDED" ||
    /Source artwork (?:cache is full|exceeds the display budget)|Active source artwork exceeds the display budget/.test(message);
  return { message, ...receipt, kind: connection ? "connection" : authorization ? "authorization" : budget ? "budget" : "unavailable", retryable: !authorization };
}
