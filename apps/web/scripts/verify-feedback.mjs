import { expect } from "@playwright/test";

// Browser acceptance of the production local mock adapter. Failures are induced
// by a real browser storage exception, without adding product test controls.
export async function verifyFeedback(browser, endpoint, _directory, check) {
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [],
    feedbackRequests = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("request", (request) => {
    if (request.method() === "POST" && /feedback/.test(request.url()))
      feedbackRequests.push(request.url());
  });
  page.setDefaultTimeout(30_000);
  const open = async () => {
    await page
      .getByRole("button", { name: "Send feedback", exact: true })
      .click();
    return page.getByRole("dialog", { name: "Send feedback", exact: true });
  };
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(
      page.getByRole("button", { name: "New project", exact: true }),
    ).toBeEnabled();
    let dialog = await open();
    await expect(
      dialog.getByText(
        "Mock feedback: saved in this browser only. No email is sent.",
      ),
    ).toBeVisible();
    await expect(dialog.getByText(/feedback@example\.invalid/)).toBeVisible();
    await expect(
      dialog.getByRole("button", { name: "Submit mock feedback", exact: true }),
    ).toBeDisabled();
    check(
      "feedback is discoverable in the workspace header and clearly labels the mock recipient",
    );

    const input = dialog.getByRole("textbox");
    await expect(input).toHaveAttribute("maxlength", "5000");
    await input.fill("x".repeat(5001));
    await expect(input).toHaveValue("x".repeat(5000));
    await input.fill("   ");
    await expect(
      dialog.getByRole("button", { name: "Submit mock feedback", exact: true }),
    ).toBeDisabled();
    await input.fill("Synthetic feedback: improve timeline snapping");
    await dialog.getByRole("button", { name: "Close", exact: true }).click();
    dialog = await open();
    await expect(dialog.getByRole("textbox")).toHaveValue(
      "Synthetic feedback: improve timeline snapping",
    );
    await page.reload();
    dialog = await open();
    await expect(dialog.getByRole("textbox")).toHaveValue(
      "Synthetic feedback: improve timeline snapping",
    );
    check(
      "feedback enforces the Classic 5000-character bound and preserves drafts through close and reload",
    );

    await page.evaluate(() => {
      const original = Storage.prototype.setItem;
      globalThis[Symbol.for("opencut.feedback.acceptance.original-set-item")] =
        original;
      Storage.prototype.setItem = function (key, value) {
        if (key === "opencut.feedback.mock-history.v1")
          throw new DOMException(
            "Synthetic quota failure",
            "QuotaExceededError",
          );
        return original.call(this, key, value);
      };
    });
    await dialog
      .getByRole("button", { name: "Submit mock feedback", exact: true })
      .click();
    await expect(dialog.getByRole("alert")).toHaveText(
      "The simulated receiver is unavailable. Your draft is preserved; try again.",
    );
    await expect(dialog.getByRole("textbox")).toHaveValue(
      "Synthetic feedback: improve timeline snapping",
    );
    await expect(
      dialog.getByRole("button", { name: "Submit mock feedback", exact: true }),
    ).toBeEnabled();
    await page.evaluate(() => {
      Storage.prototype.setItem =
        globalThis[Symbol.for("opencut.feedback.acceptance.original-set-item")];
      delete globalThis[
        Symbol.for("opencut.feedback.acceptance.original-set-item")
      ];
    });
    check(
      "actual mock receiver storage failure retains feedback and enables safe retry",
    );

    await dialog.getByRole("textbox").evaluate((element) => {
      element.form.requestSubmit();
      element.form.requestSubmit();
    });
    await expect(
      dialog.getByText("Feedback received locally (mock)", { exact: true }),
    ).toBeVisible();
    const receipt = await page.evaluate(() =>
      JSON.parse(localStorage.getItem("opencut.feedback.mock-history.v1")),
    );
    expect(receipt).toHaveLength(1);
    expect(Object.keys(receipt[0]).sort()).toEqual([
      "id",
      "message",
      "receivedAt",
      "recipient",
      "requestId",
      "simulated",
    ]);
    expect(receipt[0].message).toBe(
      "Synthetic feedback: improve timeline snapping",
    );
    expect(receipt[0].recipient).toBe("feedback@example.invalid");
    expect(receipt[0].simulated).toBe(true);
    expect(receipt[0].id).toMatch(/^mock-/);
    await expect(
      dialog.getByText(receipt[0].id, { exact: true }),
    ).toBeVisible();
    await expect(dialog.getByRole("textbox")).toHaveValue("");
    expect(feedbackRequests).toEqual([]);
    check(
      "retry returns one verifiable local mock receipt, clears the draft and sends no feedback network request",
    );

    await page.reload();
    dialog = await open();
    await dialog
      .getByRole("button", { name: "History · 1", exact: true })
      .click();
    await expect(
      dialog.getByText(receipt[0].message, { exact: true }),
    ).toBeVisible();
    await expect(
      dialog.getByText(receipt[0].id, { exact: true }),
    ).toBeVisible();
    await dialog
      .getByRole("button", {
        name: `Delete feedback ${receipt[0].id}`,
        exact: true,
      })
      .click();
    await expect(
      dialog.getByText("No simulated receipts yet.", { exact: true }),
    ).toBeVisible();
    await page.reload();
    dialog = await open();
    await dialog
      .getByRole("button", { name: "History · 0", exact: true })
      .click();
    await expect(
      dialog.getByText("No simulated receipts yet.", { exact: true }),
    ).toBeVisible();
    check(
      "mock feedback history survives reload and explicit receipt deletion persists",
    );
    expect(errors).toEqual([]);
  } finally {
    await page.close();
  }
}
