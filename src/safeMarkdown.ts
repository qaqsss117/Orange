import DOMPurify from "dompurify";
import { marked } from "marked";

export function renderSafeMarkdown(source: string): string {
  const html = marked.parse(source, {
    async: false,
    breaks: true,
    gfm: true,
  });

  return DOMPurify.sanitize(html, {
    FORBID_ATTR: ["style"],
    FORBID_TAGS: ["style"],
    USE_PROFILES: { html: true },
  });
}
