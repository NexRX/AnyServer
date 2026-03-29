import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "./test-utils/loadStylesheet";

// ─── Helpers ────────────────────────────────────────────────────────

function loadStylesheet(): string {
  return loadProjectStylesheet(__dirname);
}

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Assert that every pattern matches in `text`, with a contextual label on failure. */
function expectAllMatch(text: string, patterns: RegExp[], context: string) {
  for (const p of patterns) {
    expect(text, `${context}: missing ${p.source}`).toMatch(p);
  }
}

/** Assert the CSS contains a rule starting with `selector {` and that
 *  the rule body matches every property regex in `props`. */
function expectRule(css: string, selector: string, props: RegExp[]) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const ruleRe = new RegExp(`${escaped}\\s*\\{([^}]*)`);
  const m = css.match(ruleRe);
  expect(m, `rule ${selector} should exist`).not.toBeNull();
  const body = m![1];
  for (const p of props) {
    expect(body, `${selector}: missing ${p.source}`).toMatch(p);
  }
}

// ─── Select / Dropdown dark-theme styles (ticket 008) ───────────────

describe("select dropdown dark-theme styling (ticket 008)", () => {
  const stripped = stripComments(loadStylesheet());

  it("styles select elements with dark theme properties", () => {
    expectRule(stripped, "select", [
      /background/,
      /color/,
      /border/,
      /border-radius/,
      /font-family/,
      /-webkit-appearance:\s*none|appearance:\s*none/,
      /background-image/,
      /padding-right/,
      /cursor:\s*pointer/,
    ]);
  });

  it("has .form-group select, focus, hover, and disabled states", () => {
    // The rule is comma-separated: `.form-group select,\nselect { ... }`
    expect(stripped).toMatch(/\.form-group\s+select[\s\S]*?\{/);
    expectAllMatch(
      stripped,
      [
        /select:focus[\s\S]*?\{[^}]*border-color/,
        /select:focus[\s\S]*?\{[^}]*box-shadow/,
        /select:hover[\s\S]*?\{[^}]*border/,
        /select:disabled[\s\S]*?\{[^}]*opacity/,
        /select:disabled[\s\S]*?\{[^}]*cursor:\s*not-allowed/,
      ],
      "select states",
    );
  });

  it("styles option elements and adds transitions", () => {
    expectAllMatch(
      stripped,
      [
        /option\s*\{[^}]*background/,
        /option\s*\{[^}]*color/,
        /select\s*\{[^}]*transition[^}]*(border-color|box-shadow)/,
      ],
      "option / transition",
    );
  });

  it("uses the same tokens as text inputs (--bg-input, border, radius, focus)", () => {
    const selectRule = stripped.match(/select\s*\{([^}]*)/)?.[1] ?? "";
    const inputRule =
      stripped.match(/input\[type="text"\]\s*\{([^}]*)/)?.[1] ??
      stripped.match(/\.form-group\s+input\s*\{([^}]*)/)?.[1] ??
      "";
    // Both should reference the same custom properties
    for (const token of ["--bg-input", "--border", "--radius"]) {
      if (inputRule.includes(token)) {
        expect(selectRule, `select should use ${token}`).toContain(token);
      }
    }
  });
});

// ─── Built-in template badge & card styles (ticket 012) ─────────────

describe("built-in template badge and card styles (ticket 012)", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines badge and card CSS rules", () => {
    const rules = [
      ".template-builtin-badge",
      ".template-card-builtin",
      ".template-select-card-builtin",
    ];
    for (const r of rules) {
      expect(stripped, `missing ${r}`).toMatch(
        new RegExp(r.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "\\s*\\{"),
      );
    }
    // These two are defined as a combined comma-separated selector
    expect(stripped, "missing title-row rules").toMatch(
      /\.template-card-title-row[\s\S]*?\.template-select-card-title-row[\s\S]*?\{/,
    );
  });

  it("badge has pill shape, primary colour, uppercase, inline-flex", () => {
    expectRule(stripped, ".template-builtin-badge", [
      /border-radius:\s*999px/,
      /color.*--primary|--primary.*color/,
      /text-transform:\s*uppercase/,
      /display:\s*inline-flex/,
    ]);
  });

  it("card has distinct border, gradient background, and hover state", () => {
    expectAllMatch(
      stripped,
      [
        /\.template-card-builtin\s*\{[^}]*border/,
        /\.template-card-builtin\s*\{[^}]*background/,
        /\.template-card-builtin:hover\s*\{/,
      ],
      "built-in card",
    );
  });

  it("title row uses flex layout with centered items and gap", () => {
    // The rule is defined as a comma-separated selector:
    // .template-card-title-row, .template-select-card-title-row { ... }
    const ruleMatch = stripped.match(
      /\.template-card-title-row[\s\S]*?\{([^}]*)/,
    );
    expect(ruleMatch, "title-row rule should exist").not.toBeNull();
    const body = ruleMatch![1];
    expect(body).toMatch(/display:\s*flex/);
    expect(body).toMatch(/align-items:\s*center/);
    expect(body).toMatch(/gap/);
  });

  it("Templates and CreateServer pages render built-in markup correctly", () => {
    const templates = loadComponent("pages/Templates.tsx");
    const create = loadComponent("pages/CreateServer.tsx");
    for (const src of [templates, create]) {
      expect(src).toContain("template-builtin-badge");
      expect(src).toContain("📦");
    }
    expect(templates).toMatch(/template-card-builtin/);
    expect(templates).not.toMatch(
      /<button[^>]*>.*Delete.*<\/button>[\s\S]*?builtin/i,
    );
    expect(create).toMatch(/template-select-card-builtin/);
  });
});

// ─── ConfigEditor: no inline styles on select elements ──────────────

describe("ConfigEditor: no inline styles on select elements", () => {
  it("does not have inline style objects on <select> elements", () => {
    const source = loadComponent("components/ConfigEditor.tsx");
    const selectBlocks = source.match(/<select[\s\S]*?<\/select>/g) || [];
    for (const block of selectBlocks) {
      expect(block).not.toMatch(/style=\{/);
    }
  });
});

// ─── Required CSS custom properties ─────────────────────────────────

describe("required CSS custom properties exist in :root", () => {
  it("defines all required custom properties", () => {
    const css = loadStylesheet();
    const rootBlock = css.match(/:root\s*\{([^}]*)\}/)?.[1] ?? "";
    const requiredVars = [
      "--bg",
      "--bg-card",
      "--bg-input",
      "--border",
      "--text",
      "--text-muted",
      "--text-dim",
      "--primary",
      "--danger",
      "--success",
      "--warning",
      "--radius",
      "--mono",
    ];
    for (const v of requiredVars) {
      expect(rootBlock, `missing ${v}`).toContain(v);
    }
  });
});

// ─── CSS .loading spinner styles (ticket 002) ───────────────────────

describe("CSS .loading spinner styles (ticket 002 + ticket 010)", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines .loading with layout, and loader-spinner with SVG animation", () => {
    expectAllMatch(
      stripped,
      [
        /\.loading\s*\{/,
        /\.loader-spinner\s*\{/,
        /\.loader-arc\s*\{[^}]*animation:\s*loader-dash/,
      ],
      "loading animation",
    );
    const keyframeBlock =
      stripped.match(/@keyframes\s+loader-dash\s*\{([\s\S]*?\})\s*\}/)?.[1] ??
      "";
    expect(keyframeBlock).toMatch(/stroke-dasharray/);
  });

  it("no double ellipsis in Loader or consumer components (ticket 010)", () => {
    const loader = loadComponent("components/Loader.tsx");
    const returnBlock = loader.match(/return\s*\(([\s\S]*?)\);/)?.[1] ?? "";
    expect(returnBlock).not.toMatch(/\.{3}/);
    expect(returnBlock).not.toContain("\u2026");

    const consumers = [
      "App.tsx",
      "components/FileManager.tsx",
      "pages/AdminPanel.tsx",
      "pages/CreateServer.tsx",
      "pages/Dashboard.tsx",
      "pages/ServerDetail.tsx",
      "pages/SystemHealth.tsx",
      "pages/Templates.tsx",
    ];
    for (const file of consumers) {
      const src = loadComponent(file);
      expect(src, `${file} should use <Loader />`).toMatch(/<Loader\b/);
      expect(src, `${file} should import Loader`).toMatch(/import\s+Loader/);
      expect(
        src,
        `${file} should not use ad-hoc class="loading"`,
      ).not.toContain('class="loading"');
    }
  });

  it("defines .loading-compact with reduced padding", () => {
    expect(stripped).toMatch(/\.loading-compact\s*\{/);
    const compactPadding = stripped.match(
      /\.loading-compact\s*\{[^}]*padding:\s*([^;]+)/,
    )?.[1];
    const basePadding = stripped.match(
      /\.loading\s*\{[^}]*padding:\s*([^;]+)/,
    )?.[1];
    expect(compactPadding).toBeDefined();
    expect(basePadding).toBeDefined();
    expect(parseFloat(compactPadding!)).toBeLessThan(parseFloat(basePadding!));
  });
});

// ─── Shutdown step info CSS (ticket shutdown-panel) ─────────────────

describe("Shutdown step info CSS classes", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines all shutdown panel CSS rules", () => {
    const rules = [
      ".shutdown-panel",
      ".shutdown-panel-header",
      ".shutdown-phase-label",
      ".shutdown-countdown",
      ".shutdown-step-info",
      ".shutdown-step-counter",
      ".shutdown-step-name",
      ".shutdown-progress-bar-track",
      ".shutdown-progress-bar-fill",
      ".shutdown-step-timeout",
      ".shutdown-cancel-btn",
    ];
    for (const r of rules) {
      expect(stripped, `missing ${r}`).toMatch(
        new RegExp(r.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "\\s*\\{"),
      );
    }
  });

  it("uses tabular-nums on countdown, step-counter, and step-timeout", () => {
    for (const cls of [
      ".shutdown-countdown",
      ".shutdown-step-counter",
      ".shutdown-step-timeout",
    ]) {
      const block =
        stripped.match(
          new RegExp(cls.replace(".", "\\.") + "\\s*\\{([^}]*)"),
        )?.[1] ?? "";
      expect(block, `${cls} should use tabular-nums`).toMatch(/tabular-nums/);
    }
  });

  it("step-info uses flex, step-counter uses warning color, step-name truncates", () => {
    expectRule(stripped, ".shutdown-step-info", [/display:\s*flex/]);
    expectRule(stripped, ".shutdown-step-counter", [/--warning/]);
    expectRule(stripped, ".shutdown-step-name", [
      /text-overflow:\s*ellipsis|overflow:\s*hidden/,
    ]);
  });

  it("progress-bar-fill.sigkill uses danger color, step-timeout uses small font", () => {
    const sigkillBlock =
      stripped.match(
        /\.shutdown-progress-bar-fill\.sigkill\s*\{([^}]*)/,
      )?.[1] ?? "";
    expect(sigkillBlock).toMatch(/--danger/);
    expectRule(stripped, ".shutdown-step-timeout", [/font-size/]);
  });
});

describe("ServerDetail shutdown panel renders step info", () => {
  // ShutdownPanel was extracted from ServerDetail into its own component.
  const source = loadComponent("components/server-detail/ShutdownPanel.tsx");

  it("reads and renders all shutdown step info fields", () => {
    expectAllMatch(
      source,
      [
        /step_info/,
        /running_stop_steps/,
        /shutdown-step-info/,
        /shutdown-step-counter/,
        /shutdown-step-name/,
        /step_timeout_secs/,
        /shutdown-step-timeout/,
      ],
      "ShutdownPanel shutdown",
    );
  });

  it("imports shutdown formatting utilities", () => {
    expectAllMatch(
      source,
      [
        /computeShutdownPercent/,
        /computeGraceRemaining/,
        /computeGracePercent/,
        /formatShutdownCountdown/,
      ],
      "shutdown imports",
    );
  });

  it("handles both running_stop_steps and waiting_for_exit phases", () => {
    expect(source).toMatch(/running_stop_steps/);
    expect(source).toMatch(/waiting_for_exit/);
    expect(source).toMatch(/grace_secs/);
  });
});

// ─── GlobalErrorFallback CSS (ticket 033) ───────────────────────────

describe("GlobalErrorFallback CSS classes exist (ticket 033)", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines all error fallback CSS rules with correct properties", () => {
    expectRule(stripped, ".global-error-fallback", [
      /display:\s*flex/,
      /align-items:\s*center|justify-content:\s*center/,
      /min-height|height/,
      /background/,
    ]);
    expectRule(stripped, ".global-error-content", [
      /background/,
      /border/,
      /border-radius/,
      /box-shadow|shadow/,
    ]);
    expect(stripped).toMatch(/\.global-error-icon\s*\{[^}]*font-size/);
    expectRule(stripped, ".error-message", [
      /color.*--text-muted|--text-muted/,
    ]);
    expectRule(stripped, ".error-actions", [
      /display:\s*flex/,
      /gap/,
      /justify-content:\s*center/,
      /flex-wrap/,
    ]);
    expectRule(stripped, ".error-details", [/background/, /border/]);
    expect(stripped).toMatch(/\.error-details\s+summary\s*\{[^}]*cursor/);
    expect(stripped).toMatch(/\.error-details\s+pre\s*\{[^}]*font-family/);
    expect(stripped).toMatch(/\.error-details\s+pre\s*\{[^}]*--danger/);
  });

  it("has mobile responsive styles", () => {
    // The @media queries should reference the error classes
    const mediaBlocks = stripped.match(/@media[^{]*\{[\s\S]*?\}\s*\}/g) || [];
    const hasErrorMobile = mediaBlocks.some(
      (b) =>
        b.includes("global-error-content") ||
        b.includes("error-actions") ||
        b.includes("global-error-fallback"),
    );
    expect(hasErrorMobile).toBe(true);
  });
});

// ─── ErrorBoundary in App.tsx (ticket 033) ──────────────────────────

describe("ErrorBoundary is present in App.tsx (ticket 033)", () => {
  const source = loadComponent("App.tsx");

  it("imports and uses ErrorBoundary with GlobalErrorFallback", () => {
    expectAllMatch(
      source,
      [
        /import\s+[\s\S]*ErrorBoundary[\s\S]*from\s+["']solid-js["']/,
        /import\s+GlobalErrorFallback\s+from\s+["'][^"']*\/GlobalErrorFallback["']/,
        /<ErrorBoundary/,
        /<\/ErrorBoundary>/,
        /fallback=\{.*\(err,\s*reset\)/,
        /<GlobalErrorFallback\s+error=\{err\}\s+reset=\{reset\}/,
      ],
      "App ErrorBoundary",
    );
  });

  it("ErrorBoundary is placed below the navbar, wrapping main content", () => {
    const navIdx = source.search(/<nav\s+class="navbar"[^>]*>/);
    const ebIdx = source.indexOf("<ErrorBoundary");
    expect(navIdx).toBeGreaterThan(-1);
    expect(ebIdx).toBeGreaterThan(navIdx);

    const boundaryContent =
      source.match(/<ErrorBoundary[\s\S]*?>([\s\S]*)<\/ErrorBoundary>/)?.[1] ??
      "";
    expect(boundaryContent).toMatch(/<main[\s\S]*class=.*content/);
  });
});
