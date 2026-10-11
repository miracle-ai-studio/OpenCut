import { createElement, type ReactNode } from "react";
import { safePublicUrl } from "./public-data";
const allowed = new Set([
  "p",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "ul",
  "ol",
  "li",
  "strong",
  "b",
  "em",
  "i",
  "u",
  "s",
  "blockquote",
  "pre",
  "code",
  "br",
  "hr",
  "a",
  "img",
  "figure",
  "figcaption",
  "table",
  "thead",
  "tbody",
  "tr",
  "td",
  "th",
  "span",
  "div",
]);
const discard = new Set([
  "script",
  "style",
  "iframe",
  "object",
  "embed",
  "form",
  "input",
  "button",
  "svg",
  "math",
  "template",
]);
export function richText(source: string): ReactNode[] {
  const template = document.createElement("template");
  // The detached template is inert: resources and scripts are not activated before validation.
  template.innerHTML = source.slice(0, 500000);
  let count = 0;
  function convert(node: Node, key: string, depth: number): ReactNode {
    if (++count > 10000 || depth > 40) return null;
    if (node.nodeType === Node.TEXT_NODE) return node.textContent;
    if (!(node instanceof Element)) return null;
    const tag = node.tagName.toLowerCase();
    if (discard.has(tag)) return null;
    const children = Array.from(node.childNodes).map((child, index) =>
      convert(child, `${key}-${index}`, depth + 1),
    );
    if (!allowed.has(tag)) return children;
    const props: Record<string, unknown> = { key };
    if (tag === "a") {
      const href = safePublicUrl(node.getAttribute("href"));
      if (!href) return children;
      Object.assign(props, {
        href,
        target: "_blank",
        rel: "noopener noreferrer",
        referrerPolicy: "no-referrer",
      });
    }
    if (tag === "img") {
      const src = safePublicUrl(node.getAttribute("src"));
      if (!src) return null;
      Object.assign(props, {
        src,
        alt: node.getAttribute("alt")?.slice(0, 500) ?? "",
        loading: "lazy",
        referrerPolicy: "no-referrer",
      });
      return createElement(tag, props);
    }
    if (["br", "hr"].includes(tag)) return createElement(tag, props);
    return createElement(tag, props, children);
  }
  return Array.from(template.content.childNodes).map((node, index) =>
    convert(node, String(index), 0),
  );
}
