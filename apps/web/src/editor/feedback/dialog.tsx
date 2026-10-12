import { useEffect, useRef, useState, type FormEvent } from "react";
import { MessageSquare } from "lucide-react";
import { useLocale } from "../shared/locale";
import { Button, Field, Modal } from "../shared/ui";
import {
  FeedbackError,
  MAX_FEEDBACK_LENGTH,
  MOCK_FEEDBACK_ADDRESS,
  deleteFeedbackReceipt,
  clearFeedbackHistory,
  readFeedbackDraft,
  readFeedbackHistory,
  receiveMockFeedback,
  saveFeedbackDraft,
  submitFeedback,
  type FeedbackReceiver,
  type FeedbackReceipt,
} from "./mock-receiver";

export function FeedbackButton() {
  const { language } = useLocale();
  const [open, setOpen] = useState(false);
  const label = language === "zh" ? "发送反馈" : "Send feedback";
  return (
    <>
      <Button
        title={label}
        aria-label={label}
        icon={<MessageSquare size={16} />}
        onClick={() => setOpen(true)}
      >
        {label}
      </Button>
      {open && <FeedbackDialog onClose={() => setOpen(false)} />}
    </>
  );
}

export function FeedbackDialog({
  onClose,
  receiver = receiveMockFeedback,
  timeoutMs,
}: {
  onClose: () => void;
  receiver?: FeedbackReceiver;
  timeoutMs?: number;
}) {
  const { language } = useLocale();
  const text = (en: string, zh: string) => (language === "zh" ? zh : en);
  const [draft, setDraft] = useState(readFeedbackDraft);
  const [entries, setEntries] = useState(readFeedbackHistory);
  const [view, setView] = useState<"compose" | "history">("compose");
  const [receipt, setReceipt] = useState<FeedbackReceipt | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const operation = useRef<AbortController | null>(null);
  useEffect(() => () => operation.current?.abort(), []);
  const close = () => {
    operation.current?.abort();
    onClose();
  };
  const send = async (event: FormEvent) => {
    event.preventDefault();
    if (operation.current) return;
    const current = new AbortController();
    operation.current = current;
    setPending(true);
    setError(null);
    try {
      const received = await submitFeedback(
        draft,
        receiver,
        current.signal,
        timeoutMs,
      );
      if (current.signal.aborted) return;
      setReceipt(received);
      setEntries((previous) =>
        [
          received,
          ...previous.filter((entry) => entry.id !== received.id),
        ].slice(0, 20),
      );
      const next = { requestId: crypto.randomUUID(), message: "" };
      setDraft(next);
      saveFeedbackDraft(next);
    } catch (failure) {
      if (current.signal.aborted) return;
      const code =
        failure instanceof FeedbackError ? failure.code : "unavailable";
      setError(
        {
          invalid: text(
            "Enter feedback between 1 and 5,000 characters.",
            "请输入 1 至 5000 字的反馈。",
          ),
          timeout: text(
            "The simulated receiver timed out. Your draft is preserved; retry safely.",
            "模拟接收超时，草稿已保留，可以重试。",
          ),
          conflict: text(
            "This receipt belongs to different feedback. Edit the draft and try again.",
            "此回执对应另一条反馈，请修改草稿后重试。",
          ),
          unavailable: text(
            "The simulated receiver is unavailable. Your draft is preserved; try again.",
            "模拟接收暂不可用，草稿已保留，请重试。",
          ),
        }[code],
      );
    } finally {
      if (!current.signal.aborted) setPending(false);
      operation.current = null;
    }
  };
  return (
    <Modal title={text("Send feedback", "发送反馈")} onClose={close}>
      <p className="muted">
        {text(
          "Mock feedback: saved in this browser only. No email is sent.",
          "模拟反馈：仅保存在当前浏览器，不会发送邮件。",
        )}
      </p>
      <p className="muted">
        {text("Simulated recipient", "模拟接收地址")}：{MOCK_FEEDBACK_ADDRESS}
      </p>
      {receipt && (
        <div className="success-note" role="status">
          <strong>
            {text("Feedback received locally (mock)", "反馈已在本机模拟接收")}
          </strong>
          <p>
            {text("Receipt", "回执")}：<code>{receipt.id}</code>
          </p>
          <p>
            {new Date(receipt.receivedAt).toLocaleString(
              language === "zh" ? "zh-CN" : "en",
            )}
          </p>
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      {view === "history" ? (
        <>
          <h3>
            {text("Feedback history (this browser)", "反馈历史（当前浏览器）")}
          </h3>
          <div style={{ maxHeight: "18rem", overflowY: "auto" }}>
            {entries.length === 0 && (
              <p className="muted">
                {text("No simulated receipts yet.", "还没有模拟回执。")}
              </p>
            )}
            {entries.map((entry) => (
              <article key={entry.id}>
                <p style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>
                  {entry.message}
                </p>
                <p className="muted">
                  <code>{entry.id}</code> ·{" "}
                  {new Date(entry.receivedAt).toLocaleString(
                    language === "zh" ? "zh-CN" : "en",
                  )}
                </p>
                <Button
                  aria-label={`${text("Delete feedback", "删除反馈")} ${entry.id}`}
                  onClick={() => {
                    try {
                      setEntries(deleteFeedbackReceipt(entry.id));
                      if (receipt?.id === entry.id) setReceipt(null);
                      setError(null);
                    } catch {
                      setError(
                        text(
                          "The simulated receiver is unavailable. History was not deleted.",
                          "模拟接收暂不可用，历史记录未删除。",
                        ),
                      );
                    }
                  }}
                >
                  {text("Delete", "删除")}
                </Button>
              </article>
            ))}
          </div>
          <div className="modal-footer">
            <Button
              onClick={() => {
                try {
                  clearFeedbackHistory();
                  setEntries([]);
                  setReceipt(null);
                  setError(null);
                } catch {
                  setError(
                    text(
                      "The simulated receiver is unavailable. History was not deleted.",
                      "模拟接收暂不可用，历史记录未删除。",
                    ),
                  );
                }
              }}
            >
              {text("Clear history", "清空历史记录")}
            </Button>
            <Button onClick={() => setView("compose")}>
              {text("Back", "返回")}
            </Button>
          </div>
        </>
      ) : (
        <form onSubmit={(event) => void send(event)}>
          <Field label={text("Thoughts, bugs, ideas…", "想法、问题、建议…")}>
            <textarea
              autoFocus
              rows={6}
              maxLength={MAX_FEEDBACK_LENGTH}
              value={draft.message}
              disabled={pending}
              onChange={(event) => {
                const next = {
                  requestId: crypto.randomUUID(),
                  message: event.target.value,
                };
                setDraft(next);
                saveFeedbackDraft(next);
                setError(null);
                setReceipt(null);
              }}
            />
          </Field>
          <p className="muted">
            {draft.message.length} / {MAX_FEEDBACK_LENGTH}
          </p>
          <p className="muted">
            {text(
              "Only your message is included. Drafts and the latest 20 receipts remain in this browser.",
              "仅包含你输入的内容。草稿及最近 20 条回执保存在当前浏览器。",
            )}
          </p>
          <div className="modal-footer">
            <Button disabled={pending} onClick={() => setView("history")}>
              {text("History", "历史记录")} · {entries.length}
            </Button>
            <Button onClick={close}>{text("Cancel", "取消")}</Button>
            <Button
              type="submit"
              className="primary"
              disabled={pending || !draft.message.trim()}
            >
              {pending
                ? text("Receiving…", "接收中…")
                : text("Submit mock feedback", "提交模拟反馈")}
            </Button>
          </div>
        </form>
      )}
    </Modal>
  );
}
