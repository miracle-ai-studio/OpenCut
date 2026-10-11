import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
export async function verifySite(browser, endpoint, directory, check) {
  const context = await browser.newContext({
    locale: "en-US",
    viewport: { width: 1440, height: 1000 },
    permissions: ["clipboard-read", "clipboard-write"],
  });
  const page = await context.newPage(),
    errors = [],
    localFailures = [],
    remote = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("response", (response) => {
    if (response.status() >= 400 && response.url().startsWith(endpoint))
      localFailures.push({ url: response.url(), status: response.status() });
  });
  const sample = {
    id: "public-fixture",
    slug: "migration-test",
    title: "Community test article",
    description: "Synthetic public content for route verification.",
    content:
      '<h2>Article heading</h2><p>Public text <strong>with emphasis</strong>.</p><script>window.publicInjected=true</script><img src="javascript:alert(1)"><a href="javascript:alert(2)">Unsafe link</a>',
    coverImage: null,
    publishedAt: "2026-04-15",
    authors: [{ id: "a", name: "Test author", image: null }],
  };
  let mode = "fixture";
  await context.route("https://api.marblecms.com/**", async (route) => {
    remote.push(route.request().url());
    if (mode === "failure")
      return route.fulfill({
        status: 429,
        contentType: "application/json",
        body: "{}",
      });
    const url = new URL(route.request().url());
    if (url.pathname.endsWith("/posts/missing-post"))
      return route.fulfill({
        status: 404,
        contentType: "application/json",
        body: "{}",
      });
    const body = url.pathname.endsWith("/posts/migration-test")
      ? { post: sample }
      : {
          posts: [
            {
              ...sample,
              title:
                url.searchParams.get("page") === "2"
                  ? "Second page article"
                  : sample.title,
            },
          ],
          pagination: {
            currpage: Number(url.searchParams.get("page") ?? 1),
            totalPages: 2,
            nextPage: url.searchParams.get("page") === "2" ? null : 2,
          },
        };
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(body),
    });
  });
  await context.route(
    "https://api.github.com/repos/OpenCut-app/OpenCut/contributors?*",
    (route) =>
      route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify([
          {
            id: 1,
            login: "test-contributor",
            type: "User",
            contributions: 15,
            avatar_url: "https://avatars.githubusercontent.com/u/1",
            html_url: "https://github.com/test-contributor",
          },
          { id: 2, login: "bot", type: "Bot", contributions: 5 },
        ]),
      }),
  );
  await context.route("https://avatars.githubusercontent.com/**", (route) =>
    route.fulfill({
      status: 200,
      contentType: "image/svg+xml",
      body: '<svg xmlns="http://www.w3.org/2000/svg" width="60" height="60"><rect width="60" height="60" fill="gray"/></svg>',
    }),
  );
  try {
    await page.goto(endpoint + "/");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "The open source",
    );
    await expect(page.locator(".site-landing")).toBeVisible();
    const right = page.getByRole("slider", { name: "Trim headline →" });
    const before = Number(await right.getAttribute("aria-valuenow"));
    await right.focus();
    await page.keyboard.press("ArrowLeft");
    await expect
      .poll(async () => Number(await right.getAttribute("aria-valuenow")))
      .toBeLessThan(before);
    const left = page.getByRole("slider", { name: "Trim headline ←" }),
      box = await left.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 80, box.y + box.height / 2);
    await page.mouse.up();
    await expect
      .poll(async () => Number(await left.getAttribute("aria-valuenow")))
      .toBeGreaterThan(50);
    await page.screenshot({
      path: path.join(directory, "site-home-en-dark.png"),
      fullPage: true,
    });
    await page.getByRole("button", { name: "Change language" }).click();
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "开源的",
    );
    await page.getByRole("button", { name: "切换主题" }).click();
    await page.reload();
    await expect(page.locator("html")).toHaveAttribute("lang", "zh-CN");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
    await page.screenshot({
      path: path.join(directory, "site-home-zh-light.png"),
      fullPage: true,
    });
    await page.getByRole("link", { name: "开始剪辑", exact: true }).click();
    await expect(
      page.getByRole("button", { name: "新建项目", exact: true }),
    ).toBeEnabled();
    check(
      "Classic homepage artwork and draggable/keyboard trim handles, persistent bilingual themes and editor entry work",
    );
    await page.goto(endpoint + "/brand");
    await expect(
      page.getByRole("heading", { level: 1, name: "品牌素材" }),
    ).toBeVisible();
    const download = page.waitForEvent("download");
    await page
      .getByRole("button", { name: "下载全部素材", exact: true })
      .click();
    const archive = await download;
    const zip = path.join(directory, "brand-assets.zip");
    await archive.saveAs(zip);
    execFileSync("python3", [
      "-c",
      'import zipfile,sys; z=zipfile.ZipFile(sys.argv[1]); assert len(z.namelist())==6; assert z.testzip() is None; assert all(b"<svg" in z.read(n) for n in z.namelist())',
      zip,
    ]);
    await page
      .getByRole("button", { name: "复制 SVG", exact: true })
      .first()
      .click();
    await expect(page.getByRole("status")).toContainText("已复制 SVG");
    expect(await page.evaluate(() => navigator.clipboard.readText())).toContain(
      "<svg",
    );
    await page.screenshot({
      path: path.join(directory, "site-brand-zh-light.png"),
      fullPage: true,
    });
    await page
      .getByRole("link", { name: "OpenCut 首页" })
      .click({ button: "right" });
    await expect(page.locator(".site-brand-menu")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".site-brand-menu")).toHaveCount(0);
    await page
      .getByRole("link", { name: "OpenCut 首页" })
      .click({ button: "right" });
    await page
      .locator(".site-brand-menu")
      .getByRole("button", { name: "复制 SVG", exact: true })
      .click();
    await expect(page.locator(".site-brand-menu")).toHaveCount(0);
    expect(
      await page.evaluate(async () =>
        new DOMParser()
          .parseFromString(
            await navigator.clipboard.readText(),
            "image/svg+xml",
          )
          .documentElement.getAttribute("viewBox"),
      ),
    ).toBe("0 0 32 32");
    await page
      .getByRole("link", { name: "OpenCut 首页" })
      .click({ button: "right" });
    const symbolDownload = page.waitForEvent("download");
    await page
      .locator(".site-brand-menu")
      .getByRole("link", { name: "下载 SVG", exact: true })
      .click();
    const symbolAsset = await symbolDownload;
    expect(symbolAsset.suggestedFilename()).toBe("opencut-logo.svg");
    await symbolAsset.saveAs(path.join(directory, "header-logo.svg"));
    await page.keyboard.press("Escape");
    check(
      "Classic header logo SVG copies and downloads independently from the full brand archive",
    );
    check(
      "Classic six brand assets copy and download, valid CRC ZIP and header context menu work",
    );
    for (const [route, title] of [
      ["roadmap", "路线图"],
      ["sponsors", "赞助伙伴"],
      ["privacy", "隐私政策"],
      ["terms", "服务条款"],
      ["changelog", "更新日志"],
    ]) {
      await page.goto(endpoint + "/" + route);
      await page.reload();
      await expect(
        page.getByRole("heading", { name: title, level: 1 }),
      ).toBeVisible();
    }
    await page.goto(endpoint + "/privacy");
    await expect(page.locator(".site-prose")).toContainText(
      "清除浏览器数据不会删除项目文件",
    );
    await expect(page.locator(".site-prose")).toContainText("不等于安全擦除");
    await page.getByText("快速了解", { exact: true }).click();
    await expect(page.locator("details")).not.toHaveAttribute("open", "");
    await page.goto(endpoint + "/changelog");
    await expect(page.locator(".site-release-number")).toHaveCount(3);
    await page.locator(".site-release-number").first().click();
    await expect(page.locator(".site-changes li")).toHaveCount(52);
    await page.reload();
    await expect(page.locator(".site-changes li")).toHaveCount(52);
    await page
      .getByRole("button", { name: "复制 Markdown", exact: true })
      .click();
    await expect(page.locator(".site-copy-release + [role=status]")).toHaveText(
      "已复制！",
    );
    const markdown = await page.evaluate(() => navigator.clipboard.readText());
    expect(markdown).toContain("## Features");
    expect(markdown).toContain("<summary>Technical details</summary>");
    await expect(
      page.locator(".site-release-detail details li").first(),
    ).not.toBeVisible();
    await page.locator(".site-release-detail details summary").click();
    await expect(
      page.locator(".site-release-detail details li").first(),
    ).toBeVisible();
    await page
      .getByRole("navigation", { name: "更新导航" })
      .getByRole("link", { name: /较早更新/ })
      .click();
    await expect(page.locator(".site-changes li")).toHaveCount(19);
    await page
      .getByRole("navigation", { name: "更新导航" })
      .getByRole("link", { name: /较新更新/ })
      .click();
    await expect(page.locator(".site-changes li")).toHaveCount(52);
    check(
      "Classic release detail grouping, technical disclosure, original Markdown clipboard and older/newer navigation work",
    );
    check(
      "Classic roadmap, sponsors, adapted bilingual privacy/terms and three published releases support direct reload",
    );
    await page.goto(endpoint + "/contributors");
    await expect(page.locator(".site-contributor")).toHaveCount(1);
    await expect(page.locator(".site-stats")).toContainText("15");
    await page.goto(endpoint + "/blog");
    await expect(
      page.getByRole("heading", { name: sample.title }),
    ).toBeVisible();
    await page.getByRole("button", { name: "下一页" }).click();
    await expect(
      page.getByRole("heading", { name: "Second page article" }),
    ).toBeVisible();
    await page.getByRole("button", { name: "上一页" }).click();
    await page.getByRole("link", { name: new RegExp(sample.title) }).click();
    await expect(
      page.getByRole("heading", { name: "Article heading" }),
    ).toBeVisible();
    await expect(page.locator(".site-article script")).toHaveCount(0);
    expect(await page.evaluate(() => window.publicInjected)).toBeUndefined();
    await page.reload();
    await expect(
      page.getByRole("heading", { name: "Article heading" }),
    ).toBeVisible();
    await page.goto(endpoint + "/blog/missing-post");
    await expect(
      page.getByText("找不到这篇文章。", { exact: true }),
    ).toBeVisible();
    mode = "failure";
    await page.goto(endpoint + "/blog");
    await expect(
      page.getByText("暂时无法读取公开内容，请重试。", { exact: true }),
    ).toBeVisible();
    mode = "fixture";
    await page.getByRole("button", { name: "重试", exact: true }).click();
    await expect(
      page.getByRole("heading", { name: sample.title }),
    ).toBeVisible();
    expect(
      remote.every(
        (url) => !url.includes("project_id") && !url.includes("token"),
      ),
    ).toBe(true);
    check(
      "Public data fixtures verify contributor filtering, blog detail/pagination, sanitized content, missing articles and rate-limit retry; live providers remain separately unverified",
    );
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto(endpoint + "/");
    await page.getByRole("button", { name: "切换菜单" }).click();
    await expect(page.locator(".site-nav")).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: path.join(directory, "site-mobile-zh-light.png"),
      fullPage: true,
    });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto(endpoint + "/brand");
    await expect(
      page.getByRole("heading", { name: "品牌素材", level: 1 }),
    ).toBeVisible();
    check(
      "Public site mobile navigation avoids overflow and reduced-motion pages remain visible",
    );
    expect(errors).toEqual([]);
    expect(localFailures).toEqual([]);
  } finally {
    await context.close();
  }
}
