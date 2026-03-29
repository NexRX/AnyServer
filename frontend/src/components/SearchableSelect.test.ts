import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

function loadFile(relativePath: string): string {
  return readFileSync(resolve(__dirname, "..", relativePath), "utf-8");
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// ─── SearchableSelect component structure ───────────────────────────

describe("SearchableSelect component exists and exports correctly", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("exports SearchableSelectProps interface", () => {
    expect(source).toMatch(/export\s+interface\s+SearchableSelectProps/);
  });

  it("exports SearchableSelectOption interface", () => {
    expect(source).toMatch(/export\s+interface\s+SearchableSelectOption/);
  });

  it("exports SearchableSelect as default", () => {
    expect(source).toMatch(/export\s+default\s+SearchableSelect/);
  });

  it("SearchableSelectProps has required props: options, value, onChange", () => {
    const propsBlock =
      source.match(/interface\s+SearchableSelectProps\s*\{([\s\S]*?)\}/)?.[1] ??
      "";
    expect(propsBlock).toMatch(/options:\s*SearchableSelectOption\[\]/);
    expect(propsBlock).toMatch(/value:\s*string/);
    expect(propsBlock).toMatch(
      /onChange:\s*\(\s*value:\s*string\s*\)\s*=>\s*void/,
    );
  });

  it("SearchableSelectProps has optional props: placeholder, disabled, allowEmpty, emptyLabel, maxHeight", () => {
    const propsBlock =
      source.match(/interface\s+SearchableSelectProps\s*\{([\s\S]*?)\}/)?.[1] ??
      "";
    expect(propsBlock).toMatch(/placeholder\?:\s*string/);
    expect(propsBlock).toMatch(/disabled\?:\s*boolean/);
    expect(propsBlock).toMatch(/allowEmpty\?:\s*boolean/);
    expect(propsBlock).toMatch(/emptyLabel\?:\s*string/);
    expect(propsBlock).toMatch(/maxHeight\?:\s*string/);
  });

  it("SearchableSelectOption has value and label fields", () => {
    const optBlock =
      source.match(
        /interface\s+SearchableSelectOption\s*\{([\s\S]*?)\}/,
      )?.[1] ?? "";
    expect(optBlock).toMatch(/value:\s*string/);
    expect(optBlock).toMatch(/label:\s*string/);
  });
});

// ─── State management ───────────────────────────────────────────────

describe("SearchableSelect state management", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("tracks open/closed state with a signal", () => {
    expect(source).toMatch(/\bopen\b/);
    expect(source).toMatch(/\bsetOpen\b/);
    expect(source).toMatch(/createSignal.*false/);
  });

  it("tracks search text with a signal", () => {
    expect(source).toMatch(/\bsearch\b/);
    expect(source).toMatch(/\bsetSearch\b/);
  });

  it("tracks highlight index with a signal", () => {
    expect(source).toMatch(/\bhighlightIndex\b/);
    expect(source).toMatch(/\bsetHighlightIndex\b/);
  });

  it("resets search to empty when opening the dropdown", () => {
    const openFn =
      source.match(
        /const\s+openDropdown\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\};/,
      )?.[1] ?? "";
    expect(openFn).toMatch(/setSearch\(\s*["']["']\s*\)/);
  });

  it("resets highlight index when search text changes", () => {
    // There should be a createEffect that tracks search() and resets highlightIndex
    expect(source).toMatch(/search\(\)/);
    expect(source).toMatch(/setHighlightIndex\(\s*0\s*\)/);
  });
});

// ─── Filtering logic ────────────────────────────────────────────────

describe("SearchableSelect filtering", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("builds allOptions by prepending empty option when allowEmpty is true", () => {
    expect(source).toMatch(/allowEmpty/);
    expect(source).toMatch(/emptyLabel/);
    expect(source).toContain("— none —");
  });

  it("filters options using case-insensitive substring match on label and value", () => {
    const filteredFn =
      source.match(
        /const\s+filtered\s*=\s*\(\)[\s\S]*?=>\s*\{([\s\S]*?)\};/,
      )?.[1] ?? "";
    expect(filteredFn).toMatch(/toLowerCase\(\)/);
    expect(filteredFn).toMatch(/\.includes\(\s*q\s*\)/);
    // Should match on both label and value
    expect(filteredFn).toMatch(/o\.label/);
    expect(filteredFn).toMatch(/o\.value/);
  });

  it("returns all options when search is empty", () => {
    const filteredFn =
      source.match(
        /const\s+filtered\s*=\s*\(\)[\s\S]*?=>\s*\{([\s\S]*?)\};/,
      )?.[1] ?? "";
    expect(filteredFn).toMatch(/if\s*\(\s*!q\s*\)\s*return\s+allOptions\(\)/);
  });
});

// ─── ARIA and accessibility ─────────────────────────────────────────

describe("SearchableSelect ARIA attributes", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("trigger button has role='combobox'", () => {
    expect(source).toMatch(/role="combobox"/);
  });

  it("trigger button has aria-expanded bound to open state", () => {
    expect(source).toMatch(/aria-expanded=\{open\(\)\}/);
  });

  it("trigger button has aria-haspopup='listbox'", () => {
    expect(source).toMatch(/aria-haspopup="listbox"/);
  });

  it("options list has role='listbox'", () => {
    expect(source).toMatch(/role="listbox"/);
  });

  it("each option has role='option'", () => {
    expect(source).toMatch(/role="option"/);
  });

  it("each option has aria-selected", () => {
    expect(source).toMatch(/aria-selected=\{isSelected\(\)\}/);
  });

  it("search input has aria-label='Filter options'", () => {
    expect(source).toMatch(/aria-label="Filter options"/);
  });

  it("trigger button has aria-disabled for disabled state", () => {
    expect(source).toMatch(/aria-disabled=\{props\.disabled\}/);
  });
});

// ─── Keyboard navigation ────────────────────────────────────────────

describe("SearchableSelect keyboard navigation", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("handles ArrowDown key", () => {
    expect(source).toMatch(/case\s+["']ArrowDown["']/);
  });

  it("handles ArrowUp key", () => {
    expect(source).toMatch(/case\s+["']ArrowUp["']/);
  });

  it("handles Enter key to select highlighted option", () => {
    expect(source).toMatch(/case\s+["']Enter["']/);
    expect(source).toMatch(/selectOption/);
  });

  it("handles Escape key to close dropdown", () => {
    expect(source).toMatch(/case\s+["']Escape["']/);
    expect(source).toMatch(/closeDropdown/);
  });

  it("handles Tab key to close dropdown", () => {
    expect(source).toMatch(/case\s+["']Tab["']/);
  });

  it("ArrowDown increments highlight index with upper bound", () => {
    expect(source).toMatch(/Math\.min\(\s*prev\s*\+\s*1/);
  });

  it("ArrowUp decrements highlight index with lower bound of 0", () => {
    expect(source).toMatch(/Math\.max\(\s*prev\s*-\s*1\s*,\s*0\s*\)/);
  });

  it("scrolls highlighted option into view", () => {
    expect(source).toMatch(/scrollHighlightedIntoView/);
    expect(source).toMatch(/scrollIntoView/);
  });

  it("opens dropdown on Enter, Space, or ArrowDown when closed", () => {
    expect(source).toMatch(/e\.key\s*===\s*["']Enter["']/);
    expect(source).toMatch(/e\.key\s*===\s*["'] ["']/);
    expect(source).toMatch(/e\.key\s*===\s*["']ArrowDown["']/);
    expect(source).toMatch(/openDropdown\(\)/);
  });
});

// ─── Click-outside-to-close ─────────────────────────────────────────

describe("SearchableSelect click-outside-to-close", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("adds a mousedown event listener on document", () => {
    expect(source).toMatch(/addEventListener\(\s*["']mousedown["']/);
  });

  it("removes the listener on cleanup", () => {
    expect(source).toMatch(/removeEventListener\(\s*["']mousedown["']/);
    expect(source).toMatch(/onCleanup/);
  });

  it("checks if click target is outside the root element", () => {
    expect(source).toMatch(/rootRef/);
    expect(source).toMatch(/\.contains\(\s*e\.target/);
  });
});

// ─── Rendering ──────────────────────────────────────────────────────

describe("SearchableSelect rendering", () => {
  const source = loadComponent("SearchableSelect.tsx");

  it("renders a trigger button with the selected label", () => {
    expect(source).toMatch(/class="searchable-select-trigger"/);
    expect(source).toMatch(/selectedLabel\(\)/);
  });

  it("renders an arrow indicator that changes based on open state", () => {
    expect(source).toMatch(/class="searchable-select-trigger-arrow"/);
    expect(source).toContain("▲");
    expect(source).toContain("▼");
  });

  it("conditionally renders the dropdown panel with <Show when={open()}>", () => {
    expect(source).toMatch(/<Show\s+when=\{open\(\)\}/);
    expect(source).toMatch(/class="searchable-select-dropdown"/);
  });

  it("renders a search input inside the dropdown", () => {
    expect(source).toMatch(/class="searchable-select-search"/);
    expect(source).toMatch(/placeholder="Search…"/);
  });

  it("renders options with the searchable-select-option class", () => {
    expect(source).toMatch(/class="searchable-select-option"/);
  });

  it("highlights the selected option with a checkmark", () => {
    expect(source).toMatch(/class="searchable-select-check"/);
    expect(source).toContain("✓");
  });

  it("shows 'No matches' when filtered results are empty", () => {
    expect(source).toMatch(/class="searchable-select-no-results"/);
    expect(source).toContain("No matches");
  });

  it("applies max-height and overflow-y: auto to the options list", () => {
    expect(source).toMatch(/max-height.*maxHeight/);
    expect(source).toMatch(/overflow-y.*auto/);
  });

  it("applies highlight class via classList", () => {
    expect(source).toMatch(
      /searchable-select-option-highlighted[\s\S]*?isHighlighted/,
    );
  });

  it("applies selected class via classList", () => {
    expect(source).toMatch(
      /searchable-select-option-selected[\s\S]*?isSelected/,
    );
  });

  it("updates highlight on mouse enter", () => {
    expect(source).toMatch(/onMouseEnter.*setHighlightIndex/);
  });

  it("prevents default on mousedown for options to keep search focused", () => {
    expect(source).toMatch(/onMouseDown[\s\S]*?preventDefault/);
  });

  it("focuses the search input when dropdown opens", () => {
    expect(source).toMatch(/searchRef\?\.focus\(\)/);
  });

  it("scrolls selected option into view when dropdown opens", () => {
    expect(source).toMatch(/aria-selected="true"/);
    expect(source).toMatch(/scrollIntoView/);
  });

  it("uses requestAnimationFrame for focus and scroll operations", () => {
    expect(source).toMatch(/requestAnimationFrame/);
  });

  it("defaults maxHeight to '280px'", () => {
    expect(source).toMatch(/props\.maxHeight\s*\?\?\s*["']280px["']/);
  });
});

// ─── CSS styles for SearchableSelect ────────────────────────────────

describe("SearchableSelect CSS styles", () => {
  const css = stripComments(loadProjectStylesheet(__dirname));

  it("defines .searchable-select with relative positioning", () => {
    const block = css.match(/\.searchable-select\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/position:\s*relative/);
    expect(block).toMatch(/flex:\s*1/);
  });

  it("defines .searchable-select-trigger with flex layout and cursor pointer", () => {
    const block =
      css.match(/\.searchable-select-trigger\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/display:\s*flex/);
    expect(block).toMatch(/cursor:\s*pointer/);
    expect(block).toMatch(/background.*--bg-input/);
    expect(block).toMatch(/border.*--border/);
    expect(block).toMatch(/border-radius.*--radius-sm/);
  });

  it("defines hover, focus, and disabled states for trigger", () => {
    expect(css).toMatch(/\.searchable-select-trigger:hover/);
    expect(css).toMatch(/\.searchable-select-trigger:focus/);
    expect(css).toMatch(/\.searchable-select-trigger:disabled/);
  });

  it("defines open state trigger styling", () => {
    const block =
      css.match(/\.searchable-select-trigger-open\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/border-color.*--border-focus/);
  });

  it("defines placeholder styling for trigger", () => {
    expect(css).toMatch(/\.searchable-select-trigger-placeholder\s*\{/);
  });

  it("defines label truncation with text-overflow ellipsis", () => {
    const block =
      css.match(/\.searchable-select-trigger-label\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/text-overflow:\s*ellipsis/);
    expect(block).toMatch(/white-space:\s*nowrap/);
    expect(block).toMatch(/overflow:\s*hidden/);
  });

  it("defines dropdown with absolute positioning and z-index 1000", () => {
    const block =
      css.match(/\.searchable-select-dropdown\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/position:\s*absolute/);
    expect(block).toMatch(/z-index:\s*1000/);
    expect(block).toMatch(/background.*--bg-elevated/);
    expect(block).toMatch(/box-shadow/);
  });

  it("defines fade-in animation for dropdown", () => {
    expect(css).toMatch(/@keyframes\s+searchableSelectFadeIn/);
    expect(css).toMatch(
      /\.searchable-select-dropdown\s*\{[^}]*animation.*searchableSelectFadeIn/,
    );
  });

  it("defines search wrapper with border-bottom separator", () => {
    const block =
      css.match(/\.searchable-select-search-wrapper\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/border-bottom/);
    expect(block).toMatch(/padding/);
  });

  it("defines search input with proper theming", () => {
    const block =
      css.match(/\.searchable-select-search\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/background.*--bg-input/);
    expect(block).toMatch(/color.*--text/);
    expect(block).toMatch(/border.*--border/);
  });

  it("defines search input focus state", () => {
    expect(css).toMatch(/\.searchable-select-search:focus/);
  });

  it("defines options container with overscroll-behavior contain", () => {
    const block =
      css.match(/\.searchable-select-options\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/overflow-y:\s*auto/);
    expect(block).toMatch(/overscroll-behavior:\s*contain/);
  });

  it("defines option with flex layout and transition", () => {
    const block =
      css.match(/\.searchable-select-option\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/display:\s*flex/);
    expect(block).toMatch(/cursor:\s*pointer/);
    expect(block).toMatch(/transition/);
    expect(block).toMatch(/padding/);
  });

  it("defines highlighted option background", () => {
    const block =
      css.match(/\.searchable-select-option-highlighted\s*\{([^}]*)/)?.[1] ??
      "";
    expect(block).toMatch(/background.*--bg-hover/);
  });

  it("defines selected option with primary color", () => {
    const block =
      css.match(/\.searchable-select-option-selected\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/color.*--primary/);
    expect(block).toMatch(/font-weight.*600/);
  });

  it("defines combined selected+highlighted state", () => {
    expect(css).toMatch(
      /\.searchable-select-option-selected\.searchable-select-option-highlighted\s*\{/,
    );
  });

  it("defines check mark styling", () => {
    const block = css.match(/\.searchable-select-check\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/color.*--primary/);
  });

  it("defines no-results message styling", () => {
    const block =
      css.match(/\.searchable-select-no-results\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/text-align:\s*center/);
    expect(block).toMatch(/color.*--text-dim/);
  });
});

// ─── Integration: SearchableSelect used in consuming files ──────────

describe("SearchableSelect is used in CreateServer.tsx", () => {
  const source = loadFile("pages/CreateServer.tsx");

  it("imports SearchableSelect", () => {
    expect(source).toMatch(
      /import\s+SearchableSelect\s+from\s+["'][^"']*\/SearchableSelect["']/,
    );
  });

  it("uses <SearchableSelect> component (no native <select> for parameters)", () => {
    expect(source).toMatch(/<SearchableSelect\b/);
  });

  it("does not use native <select> for parameter dropdowns", () => {
    // The file should not have any <select> elements for parameter rendering
    // (there may be other selects for non-parameter purposes, but the param
    // rendering blocks should use SearchableSelect)
    const selectBlocks = source.match(/<select[\s\S]*?<\/select>/g) || [];
    // All native selects should have been replaced
    expect(selectBlocks.length).toBe(0);
  });

  it("passes options, value, onChange, allowEmpty, emptyLabel, and placeholder", () => {
    expect(source).toMatch(/options=\{/);
    expect(source).toMatch(/value=\{value\(\)\}/);
    expect(source).toMatch(/onChange=\{/);
    expect(source).toMatch(/allowEmpty=\{!param\.required\}/);
    expect(source).toMatch(/emptyLabel="— none —"/);
    expect(source).toMatch(/placeholder="Select or search…"/);
  });
});

describe("SearchableSelect is used in ParameterEditor.tsx", () => {
  const source = loadComponent("ParameterEditor.tsx");

  it("imports SearchableSelect", () => {
    expect(source).toMatch(
      /import\s+SearchableSelect\s+from\s+["'][^"']*\/SearchableSelect["']/,
    );
  });

  it("uses <SearchableSelect> component", () => {
    expect(source).toMatch(/<SearchableSelect\b/);
  });

  it("does not use native <select> for parameter dropdowns", () => {
    const selectBlocks = source.match(/<select[\s\S]*?<\/select>/g) || [];
    expect(selectBlocks.length).toBe(0);
  });

  it("passes allowEmpty and emptyLabel props", () => {
    expect(source).toMatch(/allowEmpty=\{!param\.required\}/);
    expect(source).toMatch(/emptyLabel="— none —"/);
  });
});

describe("SearchableSelect is used in WizardReviewStep.tsx", () => {
  const source = loadComponent("wizard/WizardReviewStep.tsx");

  it("imports SearchableSelect", () => {
    expect(source).toMatch(
      /import\s+SearchableSelect\s+from\s+["'][^"']*\/SearchableSelect["']/,
    );
  });

  it("uses <SearchableSelect> component", () => {
    expect(source).toMatch(/<SearchableSelect\b/);
  });

  it("does not use native <select> for parameter dropdowns", () => {
    const selectBlocks = source.match(/<select[\s\S]*?<\/select>/g) || [];
    expect(selectBlocks.length).toBe(0);
  });

  it("passes allowEmpty and emptyLabel props", () => {
    expect(source).toMatch(/allowEmpty=\{!param\.required\}/);
    expect(source).toMatch(/emptyLabel="— none —"/);
  });
});

describe("SearchableSelect is used in UpdateDialog.tsx", () => {
  const source = loadComponent("UpdateDialog.tsx");

  it("imports SearchableSelect", () => {
    expect(source).toMatch(
      /import\s+SearchableSelect\s+from\s+["'][^"']*\/SearchableSelect["']/,
    );
  });

  it("uses <SearchableSelect> for version selection", () => {
    expect(source).toMatch(/<SearchableSelect\b/);
  });

  it("does not use native <select> for version dropdown", () => {
    const selectBlocks = source.match(/<select[\s\S]*?<\/select>/g) || [];
    expect(selectBlocks.length).toBe(0);
  });

  it("passes version options with current/latest markers", () => {
    expect(source).toMatch(/isCurrent/);
    expect(source).toMatch(/isLatest/);
    expect(source).toMatch(/"current"/);
    expect(source).toMatch(/"latest"/);
  });

  it("passes placeholder for version selection", () => {
    expect(source).toMatch(/placeholder="Select version…"/);
  });
});
