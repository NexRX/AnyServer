import { type Component, createMemo } from "solid-js";
import { marked } from "marked";
import DOMPurify from "dompurify";

export interface MarkdownRendererProps {
  content: string;
  class?: string;
  inline?: boolean;
}

marked.setOptions({
  gfm: true,
  breaks: true,
});

DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    node.setAttribute("target", "_blank");
    node.setAttribute("rel", "noopener noreferrer");
  }
});

export function renderMarkdown(content: string, inline?: boolean): string {
  if (!content) return "";

  const raw = inline
    ? (marked.parseInline(content, { async: false }) as string)
    : (marked.parse(content, { async: false }) as string);

  return DOMPurify.sanitize(raw, {
    ALLOWED_TAGS: [
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
      "p",
      "br",
      "hr",
      "strong",
      "em",
      "b",
      "i",
      "u",
      "s",
      "del",
      "a",
      "ul",
      "ol",
      "li",
      "blockquote",
      "pre",
      "code",
      "table",
      "thead",
      "tbody",
      "tr",
      "th",
      "td",
      "img",
      "span",
      "div",
      "input",
    ],
    ALLOWED_ATTR: [
      "href",
      "title",
      "target",
      "rel",
      "alt",
      "src",
      "class",
      "type",
      "checked",
      "disabled",
    ],
    ALLOW_UNKNOWN_PROTOCOLS: false,
  });
}

const MarkdownRenderer: Component<MarkdownRendererProps> = (props) => {
  const html = createMemo(() => renderMarkdown(props.content, props.inline));

  return (
    <div
      class={`markdown-body${props.class ? ` ${props.class}` : ""}`}
      innerHTML={html()}
    />
  );
};

export default MarkdownRenderer;
