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

// ═══════════════════════════════════════════════════════════════════
// PipelineEditor collapsible behavior
// ═══════════════════════════════════════════════════════════════════

describe("PipelineEditor collapsible sections", () => {
  const source = loadComponent("./PipelineEditor.tsx");

  it("imports createSignal for collapse state", () => {
    expect(source).toMatch(/createSignal/);
  });

  it("accepts a defaultCollapsed prop", () => {
    expect(source).toMatch(/defaultCollapsed\??\s*:\s*boolean/);
  });

  it("creates a collapsed signal that uses step count as heuristic", () => {
    // When no explicit defaultCollapsed is given, collapse if steps.length === 0
    expect(source).toMatch(
      /props\.defaultCollapsed\s*\?\?\s*props\.steps\.length\s*===\s*0/,
    );
  });

  it("defaults to collapsed when empty (no steps) and no explicit prop", () => {
    // The heuristic: props.steps.length === 0 → true (collapsed)
    expect(source).toMatch(/props\.steps\.length\s*===\s*0/);
  });

  it("defaults to expanded when it has steps and no explicit prop", () => {
    // Same expression: props.steps.length === 0 → false when there ARE steps
    expect(source).toMatch(
      /props\.defaultCollapsed\s*\?\?\s*props\.steps\.length\s*===\s*0/,
    );
  });

  it("renders a chevron indicator in the header", () => {
    expect(source).toMatch(/pipeline-editor-chevron/);
  });

  it("chevron shows ▶ when collapsed and ▼ when expanded", () => {
    expect(source).toMatch(/collapsed\(\)\s*\?\s*"▶"\s*:\s*"▼"/);
  });

  it("makes the header clickable to toggle collapse", () => {
    expect(source).toMatch(/pipeline-editor-header--clickable/);
    expect(source).toMatch(/onClick.*setCollapsed\(!collapsed\(\)\)/);
  });

  it("stops propagation on header action buttons so they don't toggle", () => {
    expect(source).toMatch(
      /class="pipeline-editor-header-actions"[\s\S]*?stopPropagation/,
    );
  });

  it("wraps body content in a Show when={!collapsed()}", () => {
    expect(source).toMatch(/<Show when=\{!collapsed\(\)\}>/);
  });

  it("wraps body in a pipeline-editor-body div", () => {
    expect(source).toMatch(/class="pipeline-editor-body"/);
  });

  it("applies pipeline-editor--collapsed class when collapsed", () => {
    expect(source).toMatch(/pipeline-editor--collapsed.*collapsed\(\)/);
  });

  it("auto-expands when steps are imported", () => {
    expect(source).toMatch(
      /props\.onChange\(\[\.\.\.props\.steps, \.\.\.parsed\]\);\s*\n?\s*\/\/ Auto-expand/,
    );
    expect(source).toMatch(/setCollapsed\(false\)/);
  });

  it("only shows Import/Export/Clear buttons when expanded", () => {
    expect(source).toMatch(
      /<Show when=\{!collapsed\(\)\}>\s*\n?\s*<button[\s\S]*?Import/,
    );
  });

  it("hides the step list, add button, and description when collapsed", () => {
    expect(source).toMatch(
      /pipeline-editor-body[\s\S]*?pipeline-editor-description/,
    );
    expect(source).toMatch(/pipeline-editor-body[\s\S]*?pipeline-step-list/);
    expect(source).toMatch(/pipeline-editor-body[\s\S]*?pipeline-editor-add/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// ParameterDefinitionEditor section-level collapsible behavior
// ═══════════════════════════════════════════════════════════════════

describe("ParameterDefinitionEditor section-level collapsible", () => {
  const source = loadComponent("./ParameterDefinitionEditor.tsx");

  it("imports createSignal for collapse state", () => {
    expect(source).toMatch(/createSignal/);
  });

  it("accepts a defaultCollapsed prop", () => {
    expect(source).toMatch(/defaultCollapsed\??\s*:\s*boolean/);
  });

  it("creates a collapsed signal that uses parameter count as heuristic", () => {
    // When no explicit defaultCollapsed is given, collapse if parameters.length === 0
    expect(source).toMatch(
      /props\.defaultCollapsed\s*\?\?\s*props\.parameters\.length\s*===\s*0/,
    );
  });

  it("renders a chevron indicator in the section header", () => {
    expect(source).toMatch(/pipeline-editor-chevron/);
  });

  it("chevron shows ▶ when collapsed and ▼ when expanded", () => {
    expect(source).toMatch(/collapsed\(\)\s*\?\s*"▶"\s*:\s*"▼"/);
  });

  it("makes the section header clickable to toggle collapse", () => {
    expect(source).toMatch(/pipeline-editor-header--clickable/);
    expect(source).toMatch(/onClick.*setCollapsed\(!collapsed\(\)\)/);
  });

  it("wraps body content in a Show when={!collapsed()}", () => {
    expect(source).toMatch(/<Show when=\{!collapsed\(\)\}>/);
  });

  it("wraps body in a pipeline-editor-body div", () => {
    expect(source).toMatch(/class="pipeline-editor-body"/);
  });

  it("applies pipeline-editor--collapsed class when collapsed", () => {
    expect(source).toMatch(/pipeline-editor--collapsed.*collapsed\(\)/);
  });

  it("stops propagation on action buttons", () => {
    expect(source).toMatch(
      /class="pipeline-editor-header-actions"[\s\S]*?stopPropagation/,
    );
  });

  it("only shows Clear All button when expanded", () => {
    expect(source).toMatch(
      /<Show when=\{!collapsed\(\)\}>\s*\n?\s*<Show when=\{props\.parameters\.length > 0\}/,
    );
  });
});

// ═══════════════════════════════════════════════════════════════════
// Individual parameter card collapsible behavior
// ═══════════════════════════════════════════════════════════════════

describe("ParameterDefinitionEditor individual card collapse", () => {
  const source = loadComponent("./ParameterDefinitionEditor.tsx");

  it("creates a cardCollapsed signal per parameter card", () => {
    expect(source).toMatch(
      /const \[cardCollapsed, setCardCollapsed\]\s*=\s*createSignal\(true\)/,
    );
  });

  it("makes parameter card header clickable", () => {
    expect(source).toMatch(/parameter-def-card-header--clickable/);
  });

  it("card header has onClick to toggle cardCollapsed", () => {
    expect(source).toMatch(
      /onClick=\{.*setCardCollapsed\(!cardCollapsed\(\)\)/,
    );
  });

  it("renders a chevron in each card header", () => {
    // Uses the pipeline-step-chevron class for consistency with step editor cards
    expect(source).toMatch(
      /parameter-def-card-header[\s\S]*?pipeline-step-chevron[\s\S]*?cardCollapsed\(\)/,
    );
  });

  it("card chevron shows ▶ when collapsed and ▼ when expanded", () => {
    expect(source).toMatch(/cardCollapsed\(\)\s*\?\s*"▶"\s*:\s*"▼"/);
  });

  it("wraps card body in Show when={!cardCollapsed()}", () => {
    expect(source).toMatch(/<Show when=\{!cardCollapsed\(\)\}>/);
  });

  it("applies parameter-def-card--collapsed class when card is collapsed", () => {
    expect(source).toMatch(/parameter-def-card--collapsed.*cardCollapsed\(\)/);
  });

  it("shows param_type badge when card is collapsed", () => {
    // When collapsed, a type badge is shown in the header for quick info
    expect(source).toMatch(
      /cardCollapsed\(\)[\s\S]*?pipeline-step-type-badge[\s\S]*?param\(\)\.param_type/,
    );
  });

  it("shows default value badge when card is collapsed and default exists", () => {
    expect(source).toMatch(
      /cardCollapsed\(\)[\s\S]*?param\(\)\.default\s*!=\s*null[\s\S]*?pipeline-step-type-badge/,
    );
  });

  it("stops propagation on card action buttons so clicks don't toggle", () => {
    expect(source).toMatch(
      /parameter-def-card-header[\s\S]*?pipeline-step-header-actions[\s\S]*?stopPropagation/,
    );
  });

  it("keeps move/duplicate/remove buttons visible when collapsed", () => {
    // The action buttons are outside the cardCollapsed guard (in the header)
    expect(source).toMatch(/parameter-def-card-header[\s\S]*?handleMoveUp/);
    expect(source).toMatch(/parameter-def-card-header[\s\S]*?handleRemove/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// Start Configuration collapsible behavior in ServerDetail
// ═══════════════════════════════════════════════════════════════════

describe("ServerDetail Start Configuration collapsible section", () => {
  // Start configuration was extracted into ServerPipelinesTab sub-component.
  const source = loadComponent("server-detail/ServerPipelinesTab.tsx");

  it("creates a startConfigCollapsed signal defaulting to false (expanded by default)", () => {
    expect(source).toMatch(/startConfigCollapsed.*createSignal.*false/);
  });

  it("renders a chevron for start configuration", () => {
    expect(source).toMatch(/startConfigCollapsed\(\)\s*\?\s*"▶"\s*:\s*"▼"/);
  });

  it("makes start configuration header clickable", () => {
    expect(source).toMatch(
      /pipeline-editor-header--clickable[\s\S]*?setStartConfigCollapsed\(!startConfigCollapsed\(\)\)/,
    );
  });

  it("shows a binary path summary badge when collapsed", () => {
    expect(source).toMatch(
      /startConfigCollapsed\(\)[\s\S]*?pipeline-step-type-badge[\s\S]*?editBinary\(\)/,
    );
  });

  it("wraps start config form fields in Show when={!startConfigCollapsed()}", () => {
    expect(source).toMatch(/<Show when=\{!startConfigCollapsed\(\)\}>/);
  });

  it("wraps start config body in pipeline-editor-body div", () => {
    expect(source).toMatch(/startConfigCollapsed[\s\S]*?pipeline-editor-body/);
  });

  it("shows the Start Configuration heading text", () => {
    expect(source).toMatch(/Start Configuration/);
  });

  it("no longer uses the ▶ prefix in the heading (replaced by chevron)", () => {
    expect(source).not.toMatch(/<h3[^>]*>[\s\n]*▶ Start Configuration/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// Wizard uses expanded defaults
// ═══════════════════════════════════════════════════════════════════

describe("WizardCreateServer uses expanded defaults for editors", () => {
  const source = loadComponent("./WizardCreateServer.tsx");

  it("passes defaultCollapsed={false} to Install PipelineEditor", () => {
    expect(source).toMatch(
      /label="Install Pipeline"[\s\S]*?defaultCollapsed=\{false\}/,
    );
  });

  it("passes defaultCollapsed={false} to Update PipelineEditor", () => {
    expect(source).toMatch(
      /label="Update Pipeline"[\s\S]*?defaultCollapsed=\{false\}/,
    );
  });

  it("passes defaultCollapsed={false} to ParameterDefinitionEditor", () => {
    expect(source).toMatch(
      /ParameterDefinitionEditor[\s\S]*?defaultCollapsed=\{false\}/,
    );
  });
});

// ═══════════════════════════════════════════════════════════════════
// CSS styles for collapsible pipeline editors
// ═══════════════════════════════════════════════════════════════════

describe("CSS styles for collapsible pipeline editor sections", () => {
  const css = loadStylesheet();
  const stripped = stripComments(css);

  it("defines .pipeline-editor--collapsed modifier", () => {
    expect(stripped).toMatch(/\.pipeline-editor--collapsed/);
  });

  it("collapsed state removes margin-bottom from header", () => {
    const rule = stripped.match(
      /\.pipeline-editor--collapsed\s+\.pipeline-editor-header\s*\{([^}]+)\}/,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/margin-bottom:\s*0/);
  });

  it("defines .pipeline-editor-header--clickable with cursor pointer", () => {
    const rule = stripped.match(
      /\.pipeline-editor-header--clickable\s*\{([^}]+)\}/,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/cursor:\s*pointer/);
  });

  it("defines .pipeline-editor-header--clickable with user-select none", () => {
    const rule = stripped.match(
      /\.pipeline-editor-header--clickable\s*\{([^}]+)\}/,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/user-select:\s*none/);
  });

  it("defines hover state for clickable header", () => {
    expect(stripped).toMatch(/\.pipeline-editor-header--clickable:hover/);
  });

  it("defines .pipeline-editor-chevron with appropriate font-size", () => {
    const rule = stripped.match(/\.pipeline-editor-chevron\s*\{([^}]+)\}/);
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/font-size:\s*0\.65rem/);
  });

  it("defines .pipeline-editor-chevron with dim text color", () => {
    const rule = stripped.match(/\.pipeline-editor-chevron\s*\{([^}]+)\}/);
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/color:\s*var\(--text-dim\)/);
  });

  it("chevron changes color on header hover", () => {
    expect(stripped).toMatch(
      /\.pipeline-editor-header--clickable:hover\s+\.pipeline-editor-chevron/,
    );
  });

  it("defines .pipeline-editor-body with fadeIn animation", () => {
    const rule = stripped.match(/\.pipeline-editor-body\s*\{([^}]+)\}/);
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/animation:.*fadeIn/);
  });

  it("empty state has reduced padding compared to original 2rem", () => {
    const rule = stripped.match(/\.pipeline-editor-empty\s*\{([^}]+)\}/);
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/padding:\s*1\.25rem/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// CSS styles for collapsible parameter cards
// ═══════════════════════════════════════════════════════════════════

describe("CSS styles for collapsible parameter cards", () => {
  const css = loadStylesheet();
  const stripped = stripComments(css);

  it("defines .parameter-def-card-header--clickable with cursor pointer", () => {
    const rule = stripped.match(
      /\.parameter-def-card-header--clickable\s*\{([^}]+)\}/,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/cursor:\s*pointer/);
  });

  it("defines .parameter-def-card-header--clickable with user-select none", () => {
    const rule = stripped.match(
      /\.parameter-def-card-header--clickable\s*\{([^}]+)\}/,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/user-select:\s*none/);
  });

  it("defines hover state for clickable card header", () => {
    expect(stripped).toMatch(/\.parameter-def-card-header--clickable:hover/);
  });

  it("removes border-bottom from header when card is collapsed", () => {
    const rule = stripped.match(
      /\.parameter-def-card--collapsed\s+\.parameter-def-card-header\s*\{([^}]+)\}/,
    );
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/border-bottom:\s*none/);
  });

  it("defines fadeIn animation on parameter-def-card-body", () => {
    const rule = stripped.match(/\.parameter-def-card-body\s*\{([^}]+)\}/);
    expect(rule).not.toBeNull();
    expect(rule![1]).toMatch(/animation:.*fadeIn/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// ServerDetail pipelines tab - all sections use pipeline-section wrapper
// ═══════════════════════════════════════════════════════════════════

describe("ServerDetail pipelines tab section structure", () => {
  // Pipeline tab content was extracted into ServerPipelinesTab sub-component.
  const source = loadComponent("server-detail/ServerPipelinesTab.tsx");

  it("renders ParameterDefinitionEditor inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?ParameterDefinitionEditor/,
    );
  });

  it("renders Start Configuration inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?Start Configuration/,
    );
  });

  it("renders Stop Pipeline PipelineEditor inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?label="Stop Pipeline"/,
    );
  });

  it("renders Install Pipeline PipelineEditor inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?label="Install Pipeline"/,
    );
  });

  it("renders Update Pipeline PipelineEditor inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?label="Update Pipeline"/,
    );
  });

  it("renders Uninstall Pipeline PipelineEditor inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?label="Uninstall Pipeline"/,
    );
  });

  it("renders Pre-start Steps PipelineEditor inside a pipeline-section", () => {
    expect(source).toMatch(
      /class="pipeline-section"[\s\S]*?label="Pre-start Steps"/,
    );
  });

  it("does not pass defaultCollapsed to ServerDetail pipeline editors (uses step-aware heuristic)", () => {
    // The ServerPipelinesTab PipelineEditors should NOT have an explicit defaultCollapsed,
    // so they rely on the step-count heuristic: expanded if steps > 0, collapsed if empty.
    // Count PipelineEditor usages that have defaultCollapsed.
    const pipelineEditorUsages = source.match(/<PipelineEditor/g);
    expect(pipelineEditorUsages).not.toBeNull();
    expect(pipelineEditorUsages!.length).toBeGreaterThanOrEqual(5);
    // None of the PipelineEditor usages should have defaultCollapsed
    // (they rely on the step-aware heuristic instead)
    const editorBlocks = source.match(/<PipelineEditor[\s\S]*?\/>/g) || [];
    for (const block of editorBlocks) {
      expect(block).not.toMatch(/defaultCollapsed/);
    }
  });
});
