// Local simulation only: this module never sends mail or makes a network request.
export const MOCK_FEEDBACK_ADDRESS = "feedback@example.invalid";
export const MAX_FEEDBACK_LENGTH = 5000;
const MAX_HISTORY = 20;
export type FeedbackErrorCode =
  "invalid" | "unavailable" | "timeout" | "conflict";
export class FeedbackError extends Error {
  constructor(readonly code: FeedbackErrorCode) {
    super(code);
  }
}
export interface FeedbackDraft {
  requestId: string;
  message: string;
}
export interface FeedbackReceipt extends FeedbackDraft {
  id: string;
  receivedAt: string;
  recipient: typeof MOCK_FEEDBACK_ADDRESS;
  simulated: true;
}
export type FeedbackReceiver = (
  input: FeedbackDraft,
  signal: AbortSignal,
) => Promise<FeedbackReceipt>;
function validMessage(message: unknown): message is string {
  return (
    typeof message === "string" &&
    !!message.trim() &&
    message.length <= MAX_FEEDBACK_LENGTH
  );
}
function validDraft(value: unknown): value is FeedbackDraft {
  if (!value || typeof value !== "object") return false;
  if (Object.keys(value).sort().join(",") !== "message,requestId") return false;
  const draft = value as FeedbackDraft;
  return (
    typeof draft.requestId === "string" &&
    draft.requestId.length > 0 &&
    draft.requestId.length <= 128 &&
    typeof draft.message === "string" &&
    draft.message.length <= MAX_FEEDBACK_LENGTH
  );
}
function validReceipt(value: unknown): value is FeedbackReceipt {
  if (!value || typeof value !== "object") return false;
  if (
    Object.keys(value).sort().join(",") !==
    "id,message,receivedAt,recipient,requestId,simulated"
  )
    return false;
  const receipt = value as FeedbackReceipt;
  if (!validDraft({ requestId: receipt.requestId, message: receipt.message }))
    return false;
  return (
    validMessage(receipt.message) &&
    typeof receipt.id === "string" &&
    receipt.id.startsWith("mock-") &&
    receipt.id.length <= 128 &&
    typeof receipt.receivedAt === "string" &&
    receipt.receivedAt.length <= 32 &&
    Number.isFinite(Date.parse(receipt.receivedAt)) &&
    receipt.recipient === MOCK_FEEDBACK_ADDRESS &&
    receipt.simulated === true
  );
}
export function readFeedbackHistory(): FeedbackReceipt[] {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem("opencut.feedback.mock-history") ?? localStorage.getItem("opencut.feedback.mock-history.v1") ?? "[]",
    );
    return Array.isArray(value)
      ? value.filter(validReceipt).slice(0, MAX_HISTORY)
      : [];
  } catch {
    return [];
  }
}
function historyForMutation(): FeedbackReceipt[] {
  try {
    const raw =
      localStorage.getItem("opencut.feedback.mock-history") ?? localStorage.getItem("opencut.feedback.mock-history.v1") ?? "[]";
    if (raw.length > MAX_HISTORY * (MAX_FEEDBACK_LENGTH + 512))
      throw new FeedbackError("unavailable");
    const value: unknown = JSON.parse(raw);
    if (
      !Array.isArray(value) ||
      value.length > MAX_HISTORY ||
      !value.every(validReceipt)
    )
      throw new FeedbackError("unavailable");
    return value;
  } catch {
    // A failed read must never become an empty list that overwrites prior data.
    throw new FeedbackError("unavailable");
  }
}
export function readFeedbackDraft(): FeedbackDraft {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem("opencut.feedback.draft") ?? localStorage.getItem("opencut.feedback.draft.v1") ?? "null",
    );
    if (validDraft(value)) return value;
  } catch {
    // Draft persistence is optional; an unavailable receipt store is a submit error.
  }
  return { requestId: crypto.randomUUID(), message: "" };
}
export function saveFeedbackDraft(draft: FeedbackDraft) {
  if (!validDraft(draft)) throw new FeedbackError("invalid");
  try {
    localStorage.setItem("opencut.feedback.draft", JSON.stringify(draft));
  } catch {
    // Keep the in-memory draft usable when browser storage is unavailable.
  }
}
export const receiveMockFeedback: FeedbackReceiver = async (input, signal) => {
  if (!validDraft(input) || !validMessage(input.message))
    throw new FeedbackError("invalid");
  await Promise.resolve();
  signal.throwIfAborted();
  const entries = historyForMutation();
  // Retry deduplication covers receipts still retained in the 20-entry history.
  const previous = entries.find((entry) => entry.requestId === input.requestId);
  if (previous) {
    if (previous.message !== input.message) throw new FeedbackError("conflict");
    return previous;
  }
  const receipt: FeedbackReceipt = {
    requestId: input.requestId,
    message: input.message,
    id: `mock-${crypto.randomUUID()}`,
    receivedAt: new Date().toISOString(),
    recipient: MOCK_FEEDBACK_ADDRESS,
    simulated: true,
  };
  try {
    // No project identifiers, environment details, files, tokens or logs are collected.
    localStorage.setItem(
      "opencut.feedback.mock-history",
      JSON.stringify([receipt, ...entries].slice(0, MAX_HISTORY)),
    );
  } catch {
    throw new FeedbackError("unavailable");
  }
  return receipt;
};
export function deleteFeedbackReceipt(id: string): FeedbackReceipt[] {
  const remaining = historyForMutation().filter((entry) => entry.id !== id);
  try {
    localStorage.setItem(
      "opencut.feedback.mock-history",
      JSON.stringify(remaining),
    );
  } catch {
    throw new FeedbackError("unavailable");
  }
  return remaining;
}
export function clearFeedbackHistory() {
  try {
    // Explicit user reset may repair malformed history; submit never resets it.
    localStorage.setItem("opencut.feedback.mock-history", "[]");
  } catch {
    throw new FeedbackError("unavailable");
  }
}
export async function submitFeedback(
  input: FeedbackDraft,
  receiver: FeedbackReceiver,
  signal: AbortSignal,
  timeoutMs = 10_000,
): Promise<FeedbackReceipt> {
  if (!validDraft(input) || !validMessage(input.message))
    throw new FeedbackError("invalid");
  const operation = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let abort: (() => void) | undefined;
  try {
    signal.throwIfAborted();
    const interrupted = new Promise<never>((_resolve, reject) => {
      abort = () => {
        operation.abort(signal.reason);
        reject(signal.reason ?? new DOMException("Aborted", "AbortError"));
      };
      signal.addEventListener("abort", abort, { once: true });
      timer = setTimeout(() => {
        const error = new FeedbackError("timeout");
        operation.abort(error);
        reject(error);
      }, timeoutMs);
    });
    const receipt = await Promise.race([
      receiver(input, operation.signal),
      interrupted,
    ]);
    if (
      !validReceipt(receipt) ||
      receipt.requestId !== input.requestId ||
      receipt.message !== input.message
    )
      throw new FeedbackError("unavailable");
    return receipt;
  } finally {
    if (timer !== undefined) clearTimeout(timer);
    if (abort) signal.removeEventListener("abort", abort);
  }
}
