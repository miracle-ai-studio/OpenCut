// @vitest-environment jsdom
import { describe, it, expect } from "vitest";
import { preferences, t, elementLabel } from "./locale";
describe("presentation preferences", () => {
  it("localizes generated labels and permission errors while preserving names",()=>{
    preferences({language:"zh"});expect(elementLabel("海湾 (left)")).toBe("海湾（左段）");expect(elementLabel("My source.mp4")).toBe("My source.mp4");
    expect(t("The current credential is not authorized for this resource or action")).toContain("无权访问");
    preferences({language:"en"});expect(elementLabel("海湾 (left)")).toBe("海湾 (left)");
  });
  it("changes language and theme without storing workspace data", () => {
    preferences({ language: "zh", theme: "light" });
    expect(t("Projects")).toBe("项目");
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(window.localStorage.getItem("opencut.language")).toBe("zh");
    preferences({ language: "en", theme: "dark" });
    expect(t("Projects")).toBe("Projects");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
  it("substitutes user names without translating them", () => {
    preferences({ language: "zh" });
    expect(t("Add {value0} to timeline", { value0: "Projects" })).toBe(
      "添加 Projects 到时间线",
    );
    preferences({ language: "en" });
    expect(t("Add {value0} to timeline", { value0: "我的素材" })).toBe(
      "Add 我的素材 to timeline",
    );
  });
});
