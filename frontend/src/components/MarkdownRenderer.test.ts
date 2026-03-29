import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

// ─── Helpers ────────────────────────────────────────────────────────

function loadComponent(relativePath: string): string {
  const fullPath = resolve(__dirname, relativePath);
  return readFileSync(fullPath, "utf-8");
}

function loadStylesheet(): string {
  return loadProjectStylesheet(__dirname);
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// ─── MarkdownRenderer component source-level tests ──────────────────

describe("MarkdownRenderer component (ticket 011)", () => {
  const source = loadComponent("MarkdownRenderer.tsx");

  // ── Exports ──

  it("has a default export", () => {
    expect(source).toMatch(/export\s+default\s+MarkdownRenderer/);
  });

  it("exports the MarkdownRendererProps interface", () => {
    expect(source).toMatch(/export\s+interface\s+MarkdownRendererProps/);
  });

  it("exports the renderMarkdown function for reuse and testing", () => {
    expect(source).toMatch(/export\s+function\s+renderMarkdown/);
  });

  // ── Props ──

  it("accepts a required `content` prop", () => {
    expect(source).toMatch(/content:\s*string/);
  });

  it("accepts an optional `class` prop", () => {
    expect(source).toMatch(/class\?:\s*string/);
  });

  it("accepts an optional `inline` prop", () => {
    expect(source).toMatch(/inline\?:\s*boolean/);
  });

  // ── Dependencies ──

  it("imports marked from the marked library", () => {
    expect(source).toMatch(
      /import\s*\{[^}]*marked[^}]*\}\s*from\s*["']marked["']/,
    );
  });

  it("imports DOMPurify for sanitization", () => {
    expect(source).toMatch(/import\s+DOMPurify\s+from\s*["']dompurify["']/);
  });

  it("is a SolidJS Component (imports Component from solid-js)", () => {
    expect(source).toMatch(
      /import\s*\{[^}]*Component[^}]*\}\s*from\s*["']solid-js["']/,
    );
  });

  it("uses createMemo for reactive HTML computation", () => {
    expect(source).toMatch(
      /import\s*\{[^}]*createMemo[^}]*\}\s*from\s*["']solid-js["']/,
    );
    expect(source).toMatch(/createMemo\s*\(/);
  });

  // ── Rendering ──

  it("renders a div with the 'markdown-body' CSS class", () => {
    expect(source).toContain("markdown-body");
  });

  it("uses innerHTML to render parsed Markdown as HTML", () => {
    expect(source).toMatch(/innerHTML\s*=\s*\{/);
  });

  it("applies additional CSS classes from the class prop", () => {
    // Should conditionally concatenate props.class
    expect(source).toMatch(/props\.class/);
  });

  // ── Sanitization ──

  it("calls DOMPurify.sanitize on the parsed output", () => {
    expect(source).toMatch(/DOMPurify\.sanitize\s*\(/);
  });

  it("defines ALLOWED_TAGS for sanitization", () => {
    expect(source).toContain("ALLOWED_TAGS");
  });

  it("defines ALLOWED_ATTR for sanitization", () => {
    expect(source).toContain("ALLOWED_ATTR");
  });

  it("allows standard markdown tags: headings, paragraphs, links, code", () => {
    const allowedTagsMatch = source.match(/ALLOWED_TAGS\s*:\s*\[([\s\S]*?)\]/);
    expect(allowedTagsMatch).not.toBeNull();
    const tagsBlock = allowedTagsMatch![1];
    for (const tag of [
      "h1",
      "h2",
      "h3",
      "p",
      "a",
      "code",
      "pre",
      "strong",
      "em",
      "ul",
      "ol",
      "li",
      "blockquote",
      "br",
      "hr",
    ]) {
      expect(tagsBlock).toContain(`"${tag}"`);
    }
  });

  it("allows href, title, and target attributes on links", () => {
    const allowedAttrMatch = source.match(/ALLOWED_ATTR\s*:\s*\[([\s\S]*?)\]/);
    expect(allowedAttrMatch).not.toBeNull();
    const attrBlock = allowedAttrMatch![1];
    expect(attrBlock).toContain('"href"');
    expect(attrBlock).toContain('"title"');
    expect(attrBlock).toContain('"target"');
  });

  it("does not allow script tags in ALLOWED_TAGS", () => {
    const allowedTagsMatch = source.match(/ALLOWED_TAGS\s*:\s*\[([\s\S]*?)\]/);
    expect(allowedTagsMatch).not.toBeNull();
    const tagsBlock = allowedTagsMatch![1];
    expect(tagsBlock).not.toContain('"script"');
    expect(tagsBlock).not.toContain("'script'");
  });

  it("does not allow onclick or onerror in ALLOWED_ATTR", () => {
    const allowedAttrMatch = source.match(/ALLOWED_ATTR\s*:\s*\[([\s\S]*?)\]/);
    expect(allowedAttrMatch).not.toBeNull();
    const attrBlock = allowedAttrMatch![1];
    expect(attrBlock).not.toContain("onclick");
    expect(attrBlock).not.toContain("onerror");
  });

  // ── Inline mode ──

  it("supports inline parsing via marked.parseInline", () => {
    expect(source).toMatch(/marked\.parseInline\s*\(/);
  });

  it("uses marked.parse for block-level rendering", () => {
    expect(source).toMatch(/marked\.parse\s*\(/);
  });

  it("conditionally uses inline or block parsing based on the inline prop", () => {
    // Should check the inline parameter to decide parsing mode
    expect(source).toMatch(/inline\s*\?\s*\(?\s*\n?\s*marked\.parseInline/);
  });

  // ── Empty content handling ──

  it("returns empty string for empty content", () => {
    expect(source).toMatch(/if\s*\(\s*!content\s*\)\s*return\s*["']{2}/);
  });
});

// ─── renderMarkdown function tests ──────────────────────────────────

describe("renderMarkdown function (ticket 011)", () => {
  // We dynamically import the function so we can test it directly
  let renderMarkdown: (content: string, inline?: boolean) => string;

  it("can be imported from MarkdownRenderer", async () => {
    const mod = await import("./MarkdownRenderer");
    renderMarkdown = mod.renderMarkdown;
    expect(typeof renderMarkdown).toBe("function");
  });

  // ── Basic Markdown rendering ──

  it("renders bold text", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("**bold**");
    expect(html).toContain("<strong>bold</strong>");
  });

  it("renders italic text", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("*italic*");
    expect(html).toContain("<em>italic</em>");
  });

  it("renders links", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("[example](https://example.com)");
    expect(html).toContain('href="https://example.com"');
    expect(html).toContain("example");
  });

  it("renders inline code", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("Use `npm install`");
    expect(html).toContain("<code>npm install</code>");
  });

  it("renders code blocks", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("```\nconsole.log('hello');\n```");
    expect(html).toContain("<pre>");
    expect(html).toContain("<code>");
    expect(html).toContain("console.log");
  });

  it("renders unordered lists", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("- item 1\n- item 2\n- item 3");
    expect(html).toContain("<ul>");
    expect(html).toContain("<li>");
    expect(html).toContain("item 1");
    expect(html).toContain("item 2");
  });

  it("renders ordered lists", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("1. first\n2. second");
    expect(html).toContain("<ol>");
    expect(html).toContain("<li>");
    expect(html).toContain("first");
  });

  it("renders blockquotes", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("> This is a quote");
    expect(html).toContain("<blockquote>");
    expect(html).toContain("This is a quote");
  });

  it("renders headings", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("# Heading 1\n## Heading 2\n### Heading 3");
    expect(html).toContain("<h1");
    expect(html).toContain("Heading 1");
    expect(html).toContain("<h2");
    expect(html).toContain("Heading 2");
    expect(html).toContain("<h3");
    expect(html).toContain("Heading 3");
  });

  it("renders horizontal rules", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("above\n\n---\n\nbelow");
    expect(html).toContain("<hr");
  });

  it("renders paragraphs", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("Hello world");
    expect(html).toContain("<p>");
    expect(html).toContain("Hello world");
  });

  // ── Inline mode ──

  it("inline mode does not produce block-level <p> wrappers", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("Hello **world**", true);
    expect(html).not.toContain("<p>");
    expect(html).toContain("<strong>world</strong>");
  });

  it("inline mode renders bold and italic", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("**bold** and *italic*", true);
    expect(html).toContain("<strong>bold</strong>");
    expect(html).toContain("<em>italic</em>");
  });

  it("inline mode renders inline code", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("Use `foo` here", true);
    expect(html).toContain("<code>foo</code>");
  });

  // ── Empty / plain text ──

  it("returns empty string for empty input", async () => {
    const mod = await import("./MarkdownRenderer");
    expect(mod.renderMarkdown("")).toBe("");
  });

  it("renders plain text without breaking anything", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("Just a plain string with no markdown");
    expect(html).toContain("Just a plain string with no markdown");
  });

  // ── XSS Sanitization ──

  it("strips <script> tags from input", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown('<script>alert("xss")</script>');
    expect(html).not.toContain("<script");
    expect(html).not.toContain("</script>");
  });

  it("strips onerror attributes from img tags", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown('<img src=x onerror="alert(1)">');
    // The onerror event handler attribute should be stripped by DOMPurify
    expect(html).not.toMatch(/onerror\s*=/);
  });

  it("strips onclick attributes from elements", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown(
      '<a href="#" onclick="alert(1)">click me</a>',
    );
    expect(html).not.toContain("onclick");
  });

  it("strips javascript: URLs from links", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("[click](javascript:alert(1))");
    expect(html).not.toContain("javascript:");
  });

  it("strips <iframe> tags", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown('<iframe src="https://evil.com"></iframe>');
    expect(html).not.toContain("<iframe");
  });

  it("strips <style> tags", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown("<style>body { display: none; }</style>");
    expect(html).not.toContain("<style");
  });

  it("preserves safe content while stripping dangerous content", async () => {
    const mod = await import("./MarkdownRenderer");
    const html = mod.renderMarkdown(
      '**safe** <script>alert("xss")</script> *also safe*',
    );
    expect(html).toContain("<strong>safe</strong>");
    expect(html).toContain("<em>also safe</em>");
    expect(html).not.toContain("<script");
  });
});

// ─── CSS styles for markdown rendering (ticket 011) ─────────────────

describe("CSS .markdown-body styles (ticket 011)", () => {
  const css = loadStylesheet();
  const stripped = stripComments(css);

  // ── Base .markdown-body ──

  it("defines .markdown-body class", () => {
    expect(stripped).toMatch(/\.markdown-body\s*\{/);
  });

  it(".markdown-body has font-size", () => {
    const block = stripped.match(/\.markdown-body\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/font-size:/);
  });

  it(".markdown-body has line-height", () => {
    const block = stripped.match(/\.markdown-body\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/line-height:/);
  });

  it(".markdown-body has color from theme", () => {
    const block = stripped.match(/\.markdown-body\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/color:\s*var\(--text\)/);
  });

  // ── Heading styles ──

  it("defines heading styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+h1[\s,]/);
    expect(stripped).toMatch(/\.markdown-body\s+h2[\s,]/);
    expect(stripped).toMatch(/\.markdown-body\s+h3[\s,]/);
  });

  it("markdown headings override the global h3 text-transform", () => {
    // Global h3 has text-transform: uppercase and letter-spacing: 0.04em.
    // Markdown headings should override these for natural rendering.
    const headingRule = stripped.match(
      /\.markdown-body\s+h1[\s\S]*?\{([^}]*)\}/,
    );
    expect(headingRule).not.toBeNull();
    expect(headingRule![1]).toMatch(/text-transform:\s*none/);
  });

  it("markdown headings reset letter-spacing to normal", () => {
    const headingRule = stripped.match(
      /\.markdown-body\s+h1[\s\S]*?\{([^}]*)\}/,
    );
    expect(headingRule).not.toBeNull();
    expect(headingRule![1]).toMatch(/letter-spacing:\s*normal/);
  });

  // ── Link styles ──

  it("defines link styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+a\s*\{/);
  });

  it("markdown links use the primary color", () => {
    const linkBlock = stripped.match(/\.markdown-body\s+a\s*\{([^}]*)\}/);
    expect(linkBlock).not.toBeNull();
    expect(linkBlock![1]).toMatch(/color:\s*var\(--primary\)/);
  });

  it("markdown links have underline decoration", () => {
    const linkBlock = stripped.match(/\.markdown-body\s+a\s*\{([^}]*)\}/);
    expect(linkBlock).not.toBeNull();
    expect(linkBlock![1]).toMatch(/text-decoration:\s*underline/);
  });

  // ── Code styles ──

  it("defines inline code styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+code\s*\{/);
  });

  it("inline code uses the monospace font", () => {
    const codeBlock = stripped.match(/\.markdown-body\s+code\s*\{([^}]*)\}/);
    expect(codeBlock).not.toBeNull();
    expect(codeBlock![1]).toMatch(/font-family:\s*var\(--mono\)/);
  });

  it("inline code has background styling", () => {
    const codeBlock = stripped.match(/\.markdown-body\s+code\s*\{([^}]*)\}/);
    expect(codeBlock).not.toBeNull();
    expect(codeBlock![1]).toMatch(/background:\s*var\(--bg-input\)/);
  });

  it("defines pre (code block) styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+pre\s*\{/);
  });

  it("code blocks have overflow-x: auto for horizontal scrolling", () => {
    const preBlock = stripped.match(/\.markdown-body\s+pre\s*\{([^}]*)\}/);
    expect(preBlock).not.toBeNull();
    expect(preBlock![1]).toMatch(/overflow-x:\s*auto/);
  });

  it("code inside pre blocks has no extra background or border", () => {
    expect(stripped).toMatch(/\.markdown-body\s+pre\s+code\s*\{/);
    const preCodeBlock = stripped.match(
      /\.markdown-body\s+pre\s+code\s*\{([^}]*)\}/,
    );
    expect(preCodeBlock).not.toBeNull();
    expect(preCodeBlock![1]).toMatch(/background:\s*none/);
    expect(preCodeBlock![1]).toMatch(/border:\s*none/);
  });

  // ── Blockquote styles ──

  it("defines blockquote styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+blockquote\s*\{/);
  });

  it("blockquotes have a left border", () => {
    const bqBlock = stripped.match(
      /\.markdown-body\s+blockquote\s*\{([^}]*)\}/,
    );
    expect(bqBlock).not.toBeNull();
    expect(bqBlock![1]).toMatch(/border-left:\s*.*var\(--primary\)/);
  });

  it("blockquotes have a primary background", () => {
    const bqBlock = stripped.match(
      /\.markdown-body\s+blockquote\s*\{([^}]*)\}/,
    );
    expect(bqBlock).not.toBeNull();
    expect(bqBlock![1]).toMatch(/background:\s*var\(--primary-bg\)/);
  });

  // ── List styles ──

  it("defines list styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+ul[\s,]/);
    expect(stripped).toMatch(/\.markdown-body\s+ol[\s,]/);
  });

  it("lists have padding-left for indentation", () => {
    const listBlock = stripped.match(/\.markdown-body\s+ul[\s\S]*?\{([^}]*)\}/);
    expect(listBlock).not.toBeNull();
    expect(listBlock![1]).toMatch(/padding-left:/);
  });

  // ── Table styles ──

  it("defines table styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+table\s*\{/);
  });

  it("tables have border-collapse: collapse", () => {
    const tableBlock = stripped.match(/\.markdown-body\s+table\s*\{([^}]*)\}/);
    expect(tableBlock).not.toBeNull();
    expect(tableBlock![1]).toMatch(/border-collapse:\s*collapse/);
  });

  it("table headers have a dark background", () => {
    const thBlock = stripped.match(/\.markdown-body\s+th\s*\{([^}]*)\}/);
    expect(thBlock).not.toBeNull();
    expect(thBlock![1]).toMatch(/background:\s*var\(--bg-input\)/);
  });

  // ── Horizontal rule styles ──

  it("defines hr styles within .markdown-body", () => {
    expect(stripped).toMatch(/\.markdown-body\s+hr\s*\{/);
  });

  it("hr uses the theme border color", () => {
    const hrBlock = stripped.match(/\.markdown-body\s+hr\s*\{([^}]*)\}/);
    expect(hrBlock).not.toBeNull();
    expect(hrBlock![1]).toMatch(/border-top:\s*.*var\(--border\)/);
  });

  // ── Compact variant ──

  it("defines .markdown-body-compact class", () => {
    expect(stripped).toMatch(/\.markdown-body-compact\s*\{/);
  });

  it("compact variant has a smaller font-size", () => {
    const compactBlock = stripped.match(
      /\.markdown-body-compact\s*\{([^}]*)\}/,
    );
    expect(compactBlock).not.toBeNull();
    expect(compactBlock![1]).toMatch(/font-size:\s*0\.85rem/);
  });

  it("compact variant uses muted text color", () => {
    const compactBlock = stripped.match(
      /\.markdown-body-compact\s*\{([^}]*)\}/,
    );
    expect(compactBlock).not.toBeNull();
    expect(compactBlock![1]).toMatch(/color:\s*var\(--text-muted\)/);
  });

  // ── First/last child margins ──

  it("removes top margin on first child", () => {
    expect(stripped).toMatch(
      /\.markdown-body\s*>\s*\*:first-child\s*\{[^}]*margin-top:\s*0/,
    );
  });

  it("removes bottom margin on last child", () => {
    expect(stripped).toMatch(
      /\.markdown-body\s*>\s*\*:last-child\s*\{[^}]*margin-bottom:\s*0/,
    );
  });
});

// ─── CSS styles for markdown preview pane (ticket 011) ──────────────

describe("CSS markdown preview toggle and pane styles (ticket 011)", () => {
  const css = loadStylesheet();
  const stripped = stripComments(css);

  it("defines .markdown-preview-toggle", () => {
    expect(stripped).toMatch(/\.markdown-preview-toggle\s*\{/);
  });

  it("preview toggle uses flex layout", () => {
    const block = stripped.match(/\.markdown-preview-toggle\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/display:\s*flex/);
  });

  it("defines button styles within the toggle", () => {
    expect(stripped).toMatch(/\.markdown-preview-toggle\s+button\s*\{/);
  });

  it("defines an active state for toggle buttons", () => {
    expect(stripped).toMatch(/\.markdown-preview-toggle\s+button\.active\s*\{/);
  });

  it("active toggle button uses primary color scheme", () => {
    const block = stripped.match(
      /\.markdown-preview-toggle\s+button\.active\s*\{([^}]*)\}/,
    );
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/border-color:\s*var\(--primary\)/);
    expect(block![1]).toMatch(/color:\s*var\(--primary-hover\)/);
  });

  it("defines .markdown-preview-pane", () => {
    expect(stripped).toMatch(/\.markdown-preview-pane\s*\{/);
  });

  it("preview pane has background and border", () => {
    const block = stripped.match(/\.markdown-preview-pane\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/background:\s*var\(--bg-input\)/);
    expect(block![1]).toMatch(/border:\s*1px\s+solid\s+var\(--border\)/);
  });

  it("preview pane has min-height for usability", () => {
    const block = stripped.match(/\.markdown-preview-pane\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/min-height:/);
  });

  it("preview pane has max-height with overflow scroll", () => {
    const block = stripped.match(/\.markdown-preview-pane\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/max-height:/);
    expect(block![1]).toMatch(/overflow-y:\s*auto/);
  });

  it("defines .markdown-preview-empty for empty state", () => {
    expect(stripped).toMatch(/\.markdown-preview-empty\s*\{/);
  });

  it("empty state uses dim color and italic style", () => {
    const block = stripped.match(/\.markdown-preview-empty\s*\{([^}]*)\}/);
    expect(block).not.toBeNull();
    expect(block![1]).toMatch(/color:\s*var\(--text-dim\)/);
    expect(block![1]).toMatch(/font-style:\s*italic/);
  });
});

// ─── Templates.tsx integration (ticket 011) ─────────────────────────

describe("Templates page renders descriptions as Markdown (ticket 011)", () => {
  const source = loadComponent("../pages/Templates.tsx");

  it("imports MarkdownRenderer", () => {
    expect(source).toMatch(
      /import\s+MarkdownRenderer\s+from\s+["'][^"']*\/MarkdownRenderer["']/,
    );
  });

  it("renders template descriptions with <MarkdownRenderer>", () => {
    expect(source).toMatch(/<MarkdownRenderer\b/);
  });

  it("passes template.description as the content prop", () => {
    expect(source).toMatch(
      /<MarkdownRenderer[\s\S]*?content=\{template\.description!\}/,
    );
  });

  it("applies the compact class for card descriptions", () => {
    expect(source).toMatch(
      /<MarkdownRenderer[\s\S]*?class="markdown-body-compact[^"]*"/,
    );
  });

  it("does not render description as plain text in the template card", () => {
    // Should NOT have {template.description} as a bare text child within a <p> tag
    // after the MarkdownRenderer was added
    expect(source).not.toMatch(
      /<p\s+class="template-card-desc">\s*\{template\.description\}\s*<\/p>/,
    );
  });

  // ── Create form description preview ──

  it("has a markdown preview toggle in the create form", () => {
    expect(source).toContain("markdown-preview-toggle");
  });

  it("has Write and Preview tab buttons", () => {
    expect(source).toContain('"write"');
    expect(source).toContain('"preview"');
    // Check for the button labels
    expect(source).toMatch(/>\s*Write\s*</);
    expect(source).toMatch(/>\s*Preview\s*</);
  });

  it("has a description tab signal for write/preview state", () => {
    expect(source).toMatch(/descriptionTab/);
    expect(source).toMatch(/setDescriptionTab/);
  });

  it("shows a markdown-preview-pane in preview mode", () => {
    expect(source).toContain("markdown-preview-pane");
  });

  it("shows MarkdownRenderer inside the preview pane", () => {
    // The preview pane should contain a MarkdownRenderer rendering the current description
    expect(source).toMatch(/<MarkdownRenderer\s+content=\{templateDesc\(\)\}/);
  });

  it("has a preview empty state with 'Nothing to preview' text", () => {
    expect(source).toContain("markdown-preview-empty");
    expect(source).toContain("Nothing to preview");
  });

  it("uses a textarea instead of a single-line input for description", () => {
    // The description input should be a textarea to allow multiline markdown
    expect(source).toMatch(/<textarea[\s\S]*?onInput=\{[^}]*setTemplateDesc/);
  });

  it("labels the description field with '(Markdown supported)' hint", () => {
    expect(source).toContain("Markdown supported");
  });

  it("resets description tab to 'write' on cancel", () => {
    expect(source).toMatch(/setDescriptionTab\(\s*["']write["']\s*\)/);
  });
});

// ─── CreateServer.tsx integration (ticket 011) ──────────────────────

describe("CreateServer page renders template descriptions as Markdown (ticket 011)", () => {
  const source = loadComponent("../pages/CreateServer.tsx");

  it("imports MarkdownRenderer", () => {
    expect(source).toMatch(
      /import\s+MarkdownRenderer\s+from\s+["'][^"']*\/MarkdownRenderer["']/,
    );
  });

  it("uses <MarkdownRenderer> for template selection card descriptions", () => {
    expect(source).toMatch(/<MarkdownRenderer\b/);
    expect(source).toMatch(
      /<MarkdownRenderer[\s\S]*?content=\{template\.description!\}/,
    );
  });

  it("applies the compact class for template selection card descriptions", () => {
    expect(source).toMatch(
      /<MarkdownRenderer[\s\S]*?class="markdown-body-compact[^"]*template-select-card-desc[^"]*"/,
    );
  });

  it("uses <MarkdownRenderer> for the selected template banner description", () => {
    // After a template is selected, the banner should also use MarkdownRenderer
    expect(source).toMatch(
      /<MarkdownRenderer[\s\S]*?content=\{tmpl\(\)\.description!\}/,
    );
  });

  it("uses inline mode for the selected template banner description", () => {
    // The banner description should use inline rendering since it's compact
    expect(source).toMatch(
      /<MarkdownRenderer[\s\S]*?content=\{tmpl\(\)\.description!\}[\s\S]*?inline\b/,
    );
  });

  it("does not render description as raw text in template cards", () => {
    // Should NOT have plain {template.description} as content of a <p> element
    expect(source).not.toMatch(
      /<p\s+class="template-select-card-desc">\s*\{template\.description\}\s*<\/p>/,
    );
  });

  it("does not have hardcoded color #9ca3af for template banner description", () => {
    // The old inline style had a hardcoded color; now it uses MarkdownRenderer
    const bannerSection =
      source.match(/template-selected-banner[\s\S]{0,800}/)?.[0] ?? "";
    // Check that within the banner area, description doesn't use hardcoded color
    const descArea =
      bannerSection.match(/tmpl\(\)\.description[\s\S]{0,300}/)?.[0] ?? "";
    expect(descArea).not.toContain("#9ca3af");
  });
});

// ─── ConfigEditor.tsx integration (ticket 011) ──────────────────────

describe("ConfigEditor save-as-template dialog has Markdown support (ticket 011)", () => {
  const source = loadComponent("ConfigEditor.tsx");

  it("imports MarkdownRenderer", () => {
    expect(source).toMatch(
      /import\s+MarkdownRenderer\s+from\s+["'][^"']*\/MarkdownRenderer["']/,
    );
  });

  it("has a markdown preview toggle in the save-as-template dialog", () => {
    expect(source).toContain("markdown-preview-toggle");
  });

  it("has a template description tab signal", () => {
    expect(source).toMatch(/templateDescTab/);
    expect(source).toMatch(/setTemplateDescTab/);
  });

  it("has Write and Preview buttons", () => {
    expect(source).toMatch(/>\s*Write\s*</);
    expect(source).toMatch(/>\s*Preview\s*</);
  });

  it("shows MarkdownRenderer in the preview pane", () => {
    expect(source).toMatch(/<MarkdownRenderer\s+content=\{templateDesc\(\)\}/);
  });

  it("shows a preview pane with empty state", () => {
    expect(source).toContain("markdown-preview-pane");
    expect(source).toContain("markdown-preview-empty");
    expect(source).toContain("Nothing to preview");
  });

  it("uses a textarea for description input", () => {
    expect(source).toMatch(/<textarea[\s\S]*?onInput=\{[^}]*setTemplateDesc/);
  });

  it("labels description with '(Markdown supported)' hint", () => {
    expect(source).toContain("Markdown supported");
  });

  it("resets template description tab on cancel", () => {
    expect(source).toMatch(/setTemplateDescTab\(\s*["']write["']\s*\)/);
  });

  it("resets template description tab on successful save", () => {
    // The handleSaveAsTemplate success path should also reset the tab
    const saveHandler = source.match(
      /handleSaveAsTemplate[\s\S]*?setTemplateDialogOpen\(false\)([\s\S]*?)alert\(/,
    );
    expect(saveHandler).not.toBeNull();
    expect(saveHandler![1]).toContain("setTemplateDescTab");
  });
});

// ─── Package dependencies (ticket 011) ──────────────────────────────

describe("package.json includes markdown dependencies (ticket 011)", () => {
  const pkgPath = resolve(__dirname, "../../package.json");
  const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));

  it("has 'marked' as a dependency", () => {
    expect(pkg.dependencies).toHaveProperty("marked");
  });

  it("has 'dompurify' as a dependency", () => {
    expect(pkg.dependencies).toHaveProperty("dompurify");
  });
});
