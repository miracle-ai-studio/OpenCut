// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { preferences } from "../shared/locale";
import { FeedbackButton, FeedbackDialog } from "./dialog";
import {
  FeedbackError,
  MOCK_FEEDBACK_ADDRESS,
  readFeedbackDraft,
  readFeedbackHistory,
  receiveMockFeedback,
  submitFeedback,
} from "./mock-receiver";
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute("open");
  };
});
beforeEach(() => {
  localStorage.clear();
  preferences({ language: "en" });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
const message = () => screen.getByRole("textbox");
const send = () => screen.getByRole("button", { name: "Submit mock feedback" });
it("exposes feedback, preserves a closed draft, receives only typed content and restores its receipt history", async () => {
  const fetch = vi.spyOn(globalThis, "fetch");
  render(<FeedbackButton />);
  fireEvent.click(screen.getByRole("button", { name: "Send feedback" }));
  fireEvent.change(message(), {
    target: { value: "Timeline snapping needs improvement" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  fireEvent.click(screen.getByRole("button", { name: "Send feedback" }));
  expect((message() as HTMLTextAreaElement).value).toBe(
    "Timeline snapping needs improvement",
  );
  fireEvent.click(send());
  await screen.findByText("Feedback received locally (mock)");
  const receipt = readFeedbackHistory()[0];
  expect(receipt.message).toBe("Timeline snapping needs improvement");
  expect(receipt.recipient).toBe(MOCK_FEEDBACK_ADDRESS);
  expect(Object.keys(receipt).sort()).toEqual([
    "id",
    "message",
    "receivedAt",
    "recipient",
    "requestId",
    "simulated",
  ]);
  expect(fetch).not.toHaveBeenCalled();
  expect(readFeedbackDraft().message).toBe("");
  fireEvent.click(screen.getByRole("button", { name: "History · 1" }));
  expect(screen.getByText(receipt.message)).toBeTruthy();
  expect(screen.getAllByText(receipt.id).length).toBeGreaterThan(0);
});
it("rejects blank or oversized messages, deduplicates a retry and bounds persisted history", async () => {
  const signal = new AbortController().signal;
  for (const value of ["   ", "a".repeat(5001)]) {
    await expect(
      receiveMockFeedback({ requestId: "invalid", message: value }, signal),
    ).rejects.toMatchObject({ code: "invalid" });
  }
  const input = { requestId: "same", message: "A bug" };
  const first = await receiveMockFeedback(input, signal);
  expect(await receiveMockFeedback(input, signal)).toEqual(first);
  expect(readFeedbackHistory()).toHaveLength(1);
  await expect(
    receiveMockFeedback({ ...input, message: "Different bug" }, signal),
  ).rejects.toMatchObject({ code: "conflict" });
  for (let index = 0; index < 25; index++)
    await receiveMockFeedback(
      { requestId: `next-${index}`, message: `Feedback ${index}` },
      signal,
    );
  expect(readFeedbackHistory()).toHaveLength(20);
  expect(readFeedbackHistory()[0].message).toBe("Feedback 24");
});
it("keeps failed input, blocks concurrent submits and retries the same request safely", async () => {
  let fail: (reason: unknown) => void = () => {};
  const receiver = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise((_resolve, reject) => {
          fail = reject;
        }),
    )
    .mockImplementation(receiveMockFeedback);
  render(<FeedbackDialog onClose={vi.fn()} receiver={receiver} />);
  fireEvent.change(message(), { target: { value: "Please fix snapping" } });
  fireEvent.click(send());
  fireEvent.submit(message().closest("form")!);
  expect(receiver).toHaveBeenCalledTimes(1);
  const request = receiver.mock.calls[0][0];
  await act(async () => fail(new FeedbackError("unavailable")));
  await screen.findByRole("alert");
  expect((message() as HTMLTextAreaElement).value).toBe(request.message);
  fireEvent.click(send());
  await screen.findByText("Feedback received locally (mock)");
  expect(receiver.mock.calls[1][0]).toEqual(request);
});
it("reports receiver storage failure without clearing the draft", async () => {
  render(<FeedbackDialog onClose={vi.fn()} />);
  fireEvent.change(message(), {
    target: { value: "Storage failure must preserve this" },
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("Quota exceeded", "QuotaExceededError");
  });
  fireEvent.click(send());
  await screen.findByRole("alert");
  expect((message() as HTMLTextAreaElement).value).toBe(
    "Storage failure must preserve this",
  );
  expect(readFeedbackHistory()).toHaveLength(0);
});
it("times out and aborts the receiver while leaving feedback available to retry", async () => {
  vi.useFakeTimers();
  let receivedSignal: AbortSignal | undefined;
  const receiver = vi.fn((_input: unknown, signal: AbortSignal) => {
    receivedSignal = signal;
    return new Promise<never>(() => {});
  });
  render(
    <FeedbackDialog onClose={vi.fn()} receiver={receiver} timeoutMs={100} />,
  );
  fireEvent.change(message(), { target: { value: "Slow receiver" } });
  fireEvent.click(send());
  await act(async () => vi.advanceTimersByTimeAsync(100));
  expect(screen.getByRole("alert").textContent).toContain("timed out");
  expect(receivedSignal?.aborted).toBe(true);
  expect((message() as HTMLTextAreaElement).value).toBe("Slow receiver");
  expect((send() as HTMLButtonElement).disabled).toBe(false);
});
it("cancels pending reception when closed and rejects a mismatched acknowledgement", async () => {
  const closed = vi.fn();
  let signal: AbortSignal | undefined;
  render(
    <FeedbackDialog
      onClose={closed}
      receiver={(_input, receivedSignal) => {
        signal = receivedSignal;
        return new Promise<never>(() => {});
      }}
    />,
  );
  fireEvent.change(message(), { target: { value: "Close without sending" } });
  fireEvent.click(send());
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(signal?.aborted).toBe(true);
  expect(closed).toHaveBeenCalledOnce();
  expect(readFeedbackDraft().message).toBe("Close without sending");
  await expect(
    submitFeedback(
      { requestId: "a", message: "A" },
      async () => ({
        id: "mock-b",
        requestId: "b",
        message: "B",
        simulated: true,
        recipient: MOCK_FEEDBACK_ADDRESS,
        receivedAt: new Date().toISOString(),
      }),
      new AbortController().signal,
    ),
  ).rejects.toMatchObject({ code: "unavailable" });
});

it("deletes a chosen receipt and preserves it if the receipt store cannot write", async () => {
  const signal = new AbortController().signal;
  const entry = await receiveMockFeedback(
    { requestId: "history", message: "Feedback to remove" },
    signal,
  );
  render(<FeedbackDialog onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "History · 1" }));
  const write = vi
    .spyOn(Storage.prototype, "setItem")
    .mockImplementation(() => {
      throw new Error("storage unavailable");
    });
  fireEvent.click(
    screen.getByRole("button", { name: `Delete feedback ${entry.id}` }),
  );
  expect(screen.getByRole("alert").textContent).toContain(
    "History was not deleted",
  );
  expect(readFeedbackHistory()).toHaveLength(1);
  write.mockRestore();
  fireEvent.click(
    screen.getByRole("button", { name: `Delete feedback ${entry.id}` }),
  );
  expect(readFeedbackHistory()).toHaveLength(0);
  expect(screen.getByText("No simulated receipts yet.")).toBeTruthy();
});
it("rejects corrupted or extra-field personal data instead of restoring project or credential content", async () => {
  const signal = new AbortController().signal;
  const entry = await receiveMockFeedback(
    { requestId: "safe", message: "Safe feedback" },
    signal,
  );
  localStorage.setItem(
    "opencut.feedback.mock-history",
    JSON.stringify([{ ...entry, token: "must not restore" }, entry]),
  );
  expect(readFeedbackHistory()).toEqual([entry]);
  localStorage.setItem(
    "opencut.feedback.draft",
    JSON.stringify({
      requestId: "bad",
      message: "Draft",
      project: { assets: [] },
    }),
  );
  expect(readFeedbackDraft().message).toBe("");
  await expect(
    receiveMockFeedback(
      { requestId: "unsafe", message: "Message", token: "credential" } as never,
      signal,
    ),
  ).rejects.toMatchObject({ code: "invalid" });
  localStorage.setItem("opencut.feedback.mock-history", "not JSON");
  expect(readFeedbackHistory()).toEqual([]);
});
it("never overwrites history on a failed read and repairs corrupt history only after explicit clear", async () => {
  const signal = new AbortController().signal;
  const entry = await receiveMockFeedback(
    { requestId: "prior", message: "Existing receipt" },
    signal,
  );
  const read = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new Error("read denied");
  });
  const write = vi.spyOn(Storage.prototype, "setItem");
  await expect(
    receiveMockFeedback({ requestId: "new", message: "New feedback" }, signal),
  ).rejects.toMatchObject({ code: "unavailable" });
  expect(write).not.toHaveBeenCalled();
  read.mockRestore();
  expect(readFeedbackHistory()).toEqual([entry]);
  localStorage.setItem("opencut.feedback.mock-history", "not JSON");
  write.mockClear();
  await expect(
    receiveMockFeedback({ requestId: "new", message: "New feedback" }, signal),
  ).rejects.toMatchObject({ code: "unavailable" });
  expect(write).not.toHaveBeenCalled();
  render(<FeedbackDialog onClose={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "History · 0" }));
  fireEvent.click(screen.getByRole("button", { name: "Clear history" }));
  fireEvent.click(screen.getByRole("button", { name: "Back" }));
  fireEvent.change(message(), { target: { value: "New feedback" } });
  fireEvent.click(send());
  await screen.findByText("Feedback received locally (mock)");
  expect(readFeedbackHistory()[0].message).toBe("New feedback");
});
