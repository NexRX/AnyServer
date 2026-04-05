import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

// ─── Helpers ────────────────────────────────────────────────────────

function loadComponent(relativePath: string): string {
  const fullPath = resolve(__dirname, relativePath);
  return readFileSync(fullPath, "utf-8");
}

function loadPage(relativePath: string): string {
  const fullPath = resolve(__dirname, "../pages", relativePath);
  return readFileSync(fullPath, "utf-8");
}

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog component structure
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog component structure", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("exports a default component", () => {
    expect(source).toMatch(/export default UpdateDialog/);
  });

  it("defines the UpdateDialogProps interface", () => {
    expect(source).toMatch(/export interface UpdateDialogProps/);
  });

  it("accepts versionParam prop", () => {
    expect(source).toMatch(/versionParam:\s*ConfigParameter\s*\|\s*null/);
  });

  it("accepts parameterValues prop", () => {
    expect(source).toMatch(/parameterValues:\s*Record<string,\s*string>/);
  });

  it("accepts updateCheckResult prop", () => {
    expect(source).toMatch(
      /updateCheckResult:\s*UpdateCheckResult\s*\|\s*null/,
    );
  });

  it("accepts installedVersion prop", () => {
    expect(source).toMatch(/installedVersion:\s*string\s*\|\s*null/);
  });

  it("accepts busy prop", () => {
    expect(source).toMatch(/busy:\s*boolean/);
  });

  it("accepts onConfirm callback", () => {
    expect(source).toMatch(
      /onConfirm:\s*\(versionOverride:\s*string\s*\|\s*null\)\s*=>\s*void/,
    );
  });

  it("accepts onCancel callback", () => {
    expect(source).toMatch(/onCancel:\s*\(\)\s*=>\s*void/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog imports and dependencies
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog imports", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("imports solid-js primitives", () => {
    expect(source).toMatch(/from\s*"solid-js"/);
    expect(source).toMatch(/createSignal/);
    expect(source).toMatch(/Show/);
    expect(source).toMatch(/For/);
    expect(source).toMatch(/onMount/);
  });

  it("imports ConfigParameter type", () => {
    expect(source).toMatch(/ConfigParameter/);
  });

  it("imports FetchedOption type", () => {
    expect(source).toMatch(/FetchedOption/);
  });

  it("imports UpdateCheckResult type", () => {
    expect(source).toMatch(/UpdateCheckResult/);
  });

  it("imports fetchOptions from templates API", () => {
    expect(source).toMatch(
      /import\s*\{[^}]*fetchOptions[^}]*\}\s*from\s*["']\.\.\/api\/templates["']/,
    );
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog state management
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog state management", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("tracks versions list state", () => {
    expect(source).toMatch(/createSignal<FetchedOption\[\]>/);
  });

  it("tracks loading state", () => {
    expect(source).toMatch(/\[loading,\s*setLoading\]/);
  });

  it("tracks fetch error state", () => {
    expect(source).toMatch(/\[fetchError,\s*setFetchError\]/);
  });

  it("tracks selected version state", () => {
    expect(source).toMatch(/\[selectedVersion,\s*setSelectedVersion\]/);
  });

  it("tracks manual mode state", () => {
    expect(source).toMatch(/\[manualMode,\s*setManualMode\]/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog version display
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog version display", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("shows current version", () => {
    expect(source).toMatch(/Current version/);
    expect(source).toMatch(/currentVersion\(\)/);
  });

  it("shows latest available version when update is available", () => {
    expect(source).toMatch(/Latest available/);
    expect(source).toMatch(/latestVersion\(\)/);
  });

  it("derives current version from installedVersion or parameter values", () => {
    expect(source).toMatch(/props\.installedVersion/);
    expect(source).toMatch(/props\.parameterValues/);
  });

  it("derives latest version from update check result", () => {
    expect(source).toMatch(/props\.updateCheckResult\?\.update_available/);
    expect(source).toMatch(/props\.updateCheckResult\.latest_version/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog version selection
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog version selection", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("renders a select dropdown when versions are loaded", () => {
    expect(source).toMatch(/<select/);
    expect(source).toMatch(/selectedVersion\(\)/);
  });

  it("renders a text input in manual mode", () => {
    expect(source).toMatch(/<input[\s\S]*?type="text"/);
    expect(source).toMatch(/manualMode\(\)/);
  });

  it("provides a button to switch to manual entry", () => {
    expect(source).toMatch(/setManualMode\(true\)/);
    expect(source).toMatch(/Enter version manually/);
  });

  it("provides a refresh button for the version list", () => {
    expect(source).toMatch(/loadVersions/);
    expect(source).toMatch(/Refresh version list|↻/);
  });

  it("provides a Load versions button in manual/fallback mode", () => {
    expect(source).toMatch(/Load versions/);
  });

  it("labels dropdown options with current and latest markers", () => {
    expect(source).toMatch(/isCurrent/);
    expect(source).toMatch(/isLatest/);
    expect(source).toMatch(/"current"/);
    expect(source).toMatch(/"latest"/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog version change summary
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog version change summary", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("shows an upgrade arrow when version differs from current", () => {
    expect(source).toMatch(/isUpgrade\(\)/);
    expect(source).toMatch(/→/);
  });

  it("shows informational message when same version is selected", () => {
    expect(source).toMatch(/isCurrentSelected\(\)/);
    expect(source).toMatch(/Same version.*re-run the update pipeline/);
  });

  it("strikes through the old version in upgrade summary", () => {
    expect(source).toMatch(/text-decoration.*line-through/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog actions
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog actions", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("has a Cancel button that calls onCancel", () => {
    expect(source).toMatch(/onClick=\{props\.onCancel\}/);
    expect(source).toMatch(/>[\s\S]*Cancel[\s\S]*<\/button>/);
  });

  it("has an Update button that calls handleConfirm", () => {
    expect(source).toMatch(/onClick=\{handleConfirm\}/);
    expect(source).toMatch(/Update/);
  });

  it("disables Update button when busy", () => {
    expect(source).toMatch(/disabled=\{[\s\S]*?props\.busy/);
  });

  it("disables Update button when loading versions", () => {
    expect(source).toMatch(/disabled=\{[\s\S]*?loading\(\)/);
  });

  it("disables Update button when no version is selected", () => {
    expect(source).toMatch(/disabled=\{[\s\S]*?!selectedVersion\(\)\.trim\(\)/);
  });

  it("shows a spinner when busy", () => {
    expect(source).toMatch(/btn-spinner/);
    expect(source).toMatch(/Updating…/);
  });

  it("calls onConfirm with null when no version param exists", () => {
    expect(source).toMatch(/props\.onConfirm\(null\)/);
  });

  it("calls onConfirm with the selected version", () => {
    expect(source).toMatch(/props\.onConfirm\(version\)/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog keyboard and backdrop interactions
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog interactions", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("closes on Escape key", () => {
    expect(source).toMatch(/Escape/);
    expect(source).toMatch(/props\.onCancel\(\)/);
  });

  it("confirms on Enter key", () => {
    expect(source).toMatch(/Enter/);
    expect(source).toMatch(/handleConfirm\(\)/);
  });

  it("closes on backdrop click", () => {
    expect(source).toMatch(/handleBackdropClick/);
    expect(source).toMatch(/e\.target\s*===\s*e\.currentTarget/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog visual design
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog visual design", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("has data-testid on the overlay for E2E targeting", () => {
    expect(source).toMatch(/data-testid="update-dialog"/);
  });

  it("has data-testid on the confirm button for E2E targeting", () => {
    expect(source).toMatch(/data-testid="update-dialog-confirm"/);
  });

  it("uses a fixed overlay with backdrop blur", () => {
    expect(source).toMatch(/position.*fixed/);
    expect(source).toMatch(/backdrop-filter.*blur/);
  });

  it("has an entrance animation", () => {
    expect(source).toMatch(/update-dialog-enter/);
    expect(source).toMatch(/@keyframes update-dialog-enter/);
  });

  it("uses a gradient header", () => {
    expect(source).toMatch(/linear-gradient/);
  });

  it("has a header with update icon and title", () => {
    expect(source).toMatch(/🔄/);
    expect(source).toMatch(/Update Server/);
  });

  it("uses monospace font for version numbers", () => {
    expect(source).toMatch(/SF Mono.*Cascadia Code.*monospace/);
  });

  it("applies gradient styling to the Update button for upgrades", () => {
    expect(source).toMatch(/isUpgrade\(\)/);
    expect(source).toMatch(/linear-gradient\(135deg,\s*#3b82f6,\s*#8b5cf6\)/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog version fetching
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog version fetching", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("calls fetchOptions using the version param options_from config", () => {
    expect(source).toMatch(/fetchOptions\(\{/);
    expect(source).toMatch(/url:\s*of\.url/);
    expect(source).toMatch(/path:\s*of\.path/);
    expect(source).toMatch(/sort:\s*of\.sort/);
    expect(source).toMatch(/limit:\s*of\.limit/);
  });

  it("builds substitution params excluding the version param itself", () => {
    expect(source).toMatch(/props\.versionParam!\.name/);
    expect(source).toMatch(/subs/);
  });

  it("falls back to manual mode on fetch error", () => {
    expect(source).toMatch(/setManualMode\(true\)/);
    expect(source).toMatch(/setFetchError/);
  });

  it("shows fetch error message", () => {
    expect(source).toMatch(/fetchError\(\)/);
    expect(source).toMatch(/⚠/);
  });

  it("auto-fetches on mount when options_from is configured", () => {
    expect(source).toMatch(/onMount\(\(\)\s*=>\s*\{/);
    expect(source).toMatch(/loadVersions\(\)/);
  });

  it("defaults to manual mode when no options_from", () => {
    expect(source).toMatch(/setManualMode\(true\)/);
  });

  it("resets manualMode to false after a successful fetch with results", () => {
    // After setVersions(options), if options exist, manualMode must
    // be set back to false so the dropdown renders instead of the text input.
    expect(source).toMatch(
      /setVersions\(options\)[\s\S]*?setManualMode\(false\)/,
    );
  });
});

// ═══════════════════════════════════════════════════════════════════
// UpdateDialog no-version-param fallback
// ═══════════════════════════════════════════════════════════════════

describe("UpdateDialog no version param fallback", () => {
  const source = loadComponent("./UpdateDialog.tsx");

  it("shows a message when no version parameter exists", () => {
    expect(source).toMatch(/does not have a version parameter/);
  });

  it("indicates the update pipeline will re-run with current config", () => {
    expect(source).toMatch(/re-run with the current configuration/);
  });
});

// ═══════════════════════════════════════════════════════════════════
// ServerDetail integration with UpdateDialog
// ═══════════════════════════════════════════════════════════════════

describe("ServerDetail imports UpdateDialog", () => {
  const source = loadPage("ServerDetail.tsx");

  it("imports UpdateDialog component", () => {
    expect(source).toMatch(
      /import\s+UpdateDialog\s+from\s+["']\.\.\/components\/UpdateDialog["']/,
    );
  });
});

describe("ServerDetail update dialog state", () => {
  const source = loadPage("ServerDetail.tsx");

  it("declares updateDialogOpen signal", () => {
    expect(source).toMatch(
      /\[updateDialogOpen,\s*setUpdateDialogOpen\]\s*=\s*createSignal/,
    );
  });

  it("declares versionParam derived accessor", () => {
    expect(source).toMatch(/const versionParam\s*=\s*\(\)/);
    expect(source).toMatch(/\.is_version/);
  });
});

describe("ServerDetail handleUpdate opens dialog instead of running directly", () => {
  const source = loadPage("ServerDetail.tsx");

  it("opens the dialog when Update button is clicked", () => {
    // handleUpdate may be a one-liner arrow or a block arrow
    expect(source).toMatch(
      /const handleUpdate\s*=\s*\(\)\s*=>[\s\S]*?setUpdateDialogOpen\(true\)/,
    );
  });

  it("does NOT directly call updateServerPipeline in handleUpdate", () => {
    // handleUpdate should NOT contain doPipeline or updateServerPipeline directly
    // It may be a one-liner `() => setUpdateDialogOpen(true)` or a block `() => { ... }`
    const handleUpdateMatch = source.match(
      /const handleUpdate\s*=\s*\(\)\s*=>\s*(?:\{[^}]*\}|[^;\n]*)/,
    );
    expect(handleUpdateMatch).toBeTruthy();
    expect(handleUpdateMatch![0]).not.toMatch(/updateServerPipeline/);
    expect(handleUpdateMatch![0]).not.toMatch(/doPipeline/);
  });
});

describe("ServerDetail handleUpdateDialogConfirm", () => {
  const source = loadPage("ServerDetail.tsx");

  it("defines handleUpdateDialogConfirm function", () => {
    expect(source).toMatch(/const handleUpdateDialogConfirm/);
  });

  it("closes the dialog on confirm", () => {
    expect(source).toMatch(
      /handleUpdateDialogConfirm[\s\S]*?setUpdateDialogOpen\(false\)/,
    );
  });

  it("passes parameter_overrides when version is provided", () => {
    expect(source).toMatch(/parameter_overrides/);
    expect(source).toMatch(/versionOverride/);
  });

  it("calls updateServerPipeline on confirm", () => {
    expect(source).toMatch(
      /handleUpdateDialogConfirm[\s\S]*?updateServerPipeline/,
    );
  });
});

describe("ServerDetail handleUpdateDialogCancel", () => {
  const source = loadPage("ServerDetail.tsx");

  it("defines handleUpdateDialogCancel function or inline cancel handler", () => {
    // May be a named function or an inline arrow on the onCancel prop
    expect(source).toMatch(
      /handleUpdateDialogCancel|onCancel=\{.*setUpdateDialogOpen\(false\)/,
    );
  });

  it("closes the dialog on cancel", () => {
    // The cancel handler calls setUpdateDialogOpen(false), either named or inline
    expect(source).toMatch(/setUpdateDialogOpen\(false\)/);
  });
});

describe("ServerDetail renders UpdateDialog", () => {
  const source = loadPage("ServerDetail.tsx");

  it("conditionally renders UpdateDialog when updateDialogOpen is true", () => {
    expect(source).toMatch(/Show\s+when=\{updateDialogOpen\(\)\}/);
    expect(source).toMatch(/<UpdateDialog/);
  });

  it("passes versionParam prop", () => {
    expect(source).toMatch(/versionParam=\{versionParam\(\)\}/);
  });

  it("passes parameterValues prop", () => {
    expect(source).toMatch(/parameterValues=\{/);
  });

  it("passes updateCheckResult prop", () => {
    // May use updateCheckResult() or updateCheck.updateCheckResult()
    expect(source).toMatch(/updateCheckResult=\{.*updateCheckResult\(\)\}/);
  });

  it("passes installedVersion prop", () => {
    expect(source).toMatch(/installedVersion=\{/);
  });

  it("passes busy prop tied to activeAction", () => {
    expect(source).toMatch(/busy=\{activeAction\(\)\s*===\s*"update"\}/);
  });

  it("passes onConfirm callback", () => {
    expect(source).toMatch(/onConfirm=\{handleUpdateDialogConfirm\}/);
  });

  it("passes onCancel callback", () => {
    // May use a named handleUpdateDialogCancel or an inline arrow
    expect(source).toMatch(
      /onCancel=\{(?:handleUpdateDialogCancel|.*setUpdateDialogOpen\(false\).*)\}/,
    );
  });
});
