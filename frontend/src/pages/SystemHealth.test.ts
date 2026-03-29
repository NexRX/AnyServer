import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

function loadStylesheet(): string {
  return loadProjectStylesheet(__dirname);
}

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

// ─── SystemHealth page source checks ────────────────────────────────

describe("SystemHealth page structure", () => {
  const source = loadComponent("SystemHealth.tsx");

  it("exports a default component that fetches and polls system health", () => {
    expect(source).toMatch(/export default SystemHealth/);
    expect(source).toMatch(/getSystemHealth/);
    expect(source).toMatch(/setInterval\(fetchHealth/);
    expect(source).toMatch(/onCleanup\(\(\) => clearInterval\(interval\)\)/);
  });

  it("uses the system-health CSS class and renders the page header", () => {
    expect(source).toMatch(/class="system-health"/);
    expect(source).toMatch(/System Health<\/h1>/);
  });

  it("renders CPU, Memory, Disk, and Network section components", () => {
    for (const section of [
      "CpuSection",
      "MemorySection",
      "DiskSection",
      "NetworkSection",
    ]) {
      expect(source, `missing <${section} />`).toMatch(
        new RegExp(`<${section}`),
      );
    }
  });

  it("displays hostname, uptime, loading state, and error state", () => {
    expect(source).toMatch(/health-hostname/);
    expect(source).toMatch(/formatUptime/);
    expect(source).toMatch(/Loading system metrics/);
    expect(source).toMatch(/Failed to load system health/);
  });
});

// ─── Formatting helpers logic (via source inspection) ───────────────

describe("SystemHealth formatting helpers", () => {
  const source = loadComponent("SystemHealth.tsx");
  const formatUtils = loadComponent("../utils/format.ts");

  it("defines formatBytes, formatUptime, and thresholdClass helpers", () => {
    // formatBytes and formatUptime are defined locally (wrapping shared utils)
    expect(source).toMatch(/function formatBytes\(bytes: number\): string/);
    expect(source).toMatch(/if \(bytes === 0\) return "0 B"/);
    expect(source).toMatch(/function formatUptime\(seconds: number\): string/);
    // thresholdClass is imported from utils/format
    expect(source).toMatch(/thresholdClass/);
    expect(formatUtils).toMatch(
      /function thresholdClass\(p(?:ct|ercent): number\): string/,
    );
  });

  it("thresholdClass returns critical (>=90), warning (>=70), and ok", () => {
    expect(formatUtils).toMatch(
      /if \(p(?:ct|ercent) >= 90\) return "critical"/,
    );
    expect(formatUtils).toMatch(/if \(p(?:ct|ercent) >= 70\) return "warning"/);
    expect(formatUtils).toMatch(/return "ok"/);
  });
});

// ─── ProgressBar sub-component ──────────────────────────────────────

describe("ProgressBar sub-component", () => {
  const source = loadComponent("SystemHealth.tsx");

  it("defines ProgressBar that clamps percentage and applies threshold class", () => {
    expect(source).toMatch(/const ProgressBar/);
    expect(source).toMatch(/Math\.max\(0, Math\.min\(100, props\.percent\)\)/);
    expect(source).toMatch(/health-bar-fill health-bar-\$/);
    expect(source).toMatch(/width:.*clamped\(\)/);
  });
});

// ─── API client integration ─────────────────────────────────────────

describe("SystemHealth API client", () => {
  it("api/system.ts exports getSystemHealth calling /system/health", () => {
    const apiSource = loadComponent("../api/system.ts");
    expect(apiSource).toMatch(
      /export function getSystemHealth\(\): Promise<SystemHealth>/,
    );
    expect(apiSource).toMatch(/\/system\/health/);
  });

  it("api/client.ts re-exports getSystemHealth", () => {
    expect(loadComponent("../api/client.ts")).toMatch(/getSystemHealth/);
  });
});

// ─── Type bindings ──────────────────────────────────────────────────

describe("SystemHealth type bindings", () => {
  const bindings = loadComponent("../types/bindings.ts");

  it("re-exports all system health types", () => {
    for (const t of [
      "SystemHealth",
      "CpuMetrics",
      "MemoryMetrics",
      "DiskMetrics",
      "NetworkMetrics",
    ]) {
      expect(bindings, `missing ${t} re-export`).toMatch(
        new RegExp(`export type \\{ ${t} \\} from "./generated/${t}"`),
      );
    }
  });
});

// ─── Generated TS types use number, not bigint ──────────────────────

describe("Generated TS types use number instead of bigint", () => {
  const typesToCheck: [string, RegExp[]][] = [
    [
      "MemoryMetrics",
      [/total_bytes: number/, /used_bytes: number/, /available_bytes: number/],
    ],
    [
      "DiskMetrics",
      [/total_bytes: number/, /used_bytes: number/, /free_bytes: number/],
    ],
    ["NetworkMetrics", [/rx_bytes: number/, /tx_bytes: number/]],
    ["SystemHealth", [/uptime_secs: number/]],
  ];

  for (const [typeName, patterns] of typesToCheck) {
    it(`${typeName} uses number (not bigint) for numeric fields`, () => {
      const source = loadComponent(`../types/generated/${typeName}.ts`);
      expect(source).not.toMatch(/bigint/);
      for (const p of patterns) {
        expect(source, `${typeName}: missing ${p.source}`).toMatch(p);
      }
    });
  }
});

// ─── CSS styles for System Health dashboard ─────────────────────────

describe("System Health CSS styles", () => {
  const stripped = stripComments(loadStylesheet());

  it("defines layout classes: .system-health and .health-grid (2-col grid)", () => {
    expect(stripped).toMatch(/\.system-health\s*\{/);
    expect(stripped).toMatch(/\.health-grid\s*\{[^}]*display:\s*grid/);
    expect(stripped).toMatch(
      /\.health-grid\s*\{[^}]*grid-template-columns:\s*repeat\(2,\s*1fr\)/,
    );
  });

  it("styles .health-card with dark background, border, and border-radius", () => {
    const block = stripped.match(/\.health-card\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/background:\s*var\(--bg-card\)/);
    expect(block).toMatch(/border:\s*1px solid var\(--border\)/);
    expect(block).toMatch(/border-radius:\s*var\(--radius\)/);
  });

  it("defines card sub-elements: header, body, and badge", () => {
    for (const cls of [
      ".health-card-header",
      ".health-card-body",
      ".health-card-badge",
    ]) {
      expect(stripped, `missing ${cls}`).toMatch(
        new RegExp(cls.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "\\s*\\{"),
      );
    }
  });

  it("defines progress bar track, fill with transition, and color variants (ok/warning/critical)", () => {
    expect(stripped).toMatch(
      /\.health-bar-track\s*\{[^}]*background:\s*var\(--bg-input\)/,
    );
    expect(stripped).toMatch(/\.health-bar-fill\s*\{[^}]*transition:/);
    expect(stripped).toMatch(
      /\.health-bar-ok\s*\{[^}]*background:\s*var\(--success\)/,
    );
    expect(stripped).toMatch(
      /\.health-bar-warning\s*\{[^}]*background:\s*var\(--warning\)/,
    );
    expect(stripped).toMatch(
      /\.health-bar-critical\s*\{[^}]*background:\s*var\(--danger\)/,
    );
    expect(stripped).toMatch(/\.health-bar-track-sm\s*\{/);
  });

  it("defines per-core grid, core items, and monospace core labels", () => {
    expect(stripped).toMatch(/\.health-core-grid\s*\{[^}]*display:\s*grid/);
    expect(stripped).toMatch(/\.health-core-item\s*\{[^}]*display:\s*flex/);
    expect(stripped).toMatch(
      /\.health-core-label\s*\{[^}]*font-family:\s*var\(--mono\)/,
    );
  });

  it("defines network row grid with RX (success) and TX (primary) colors", () => {
    expect(stripped).toMatch(/\.health-network-row\s*\{[^}]*display:\s*grid/);
    expect(stripped).toMatch(
      /\.health-network-col-rx\s*\{[^}]*color:\s*var\(--success\)/,
    );
    expect(stripped).toMatch(
      /\.health-network-col-tx\s*\{[^}]*color:\s*var\(--primary\)/,
    );
  });

  it("defines hostname with monospace font", () => {
    expect(stripped).toMatch(
      /\.health-hostname\s*\{[^}]*font-family:\s*var\(--mono\)/,
    );
  });

  it("has a responsive breakpoint that collapses health-grid to 1 column", () => {
    expect(stripped).toMatch(
      /\.health-grid\s*\{[^}]*grid-template-columns:\s*1fr/,
    );
  });
});

// ─── Route and navigation integration ───────────────────────────────

describe("SystemHealth route and navigation", () => {
  it("index.tsx includes the /health route mapped to SystemHealth", () => {
    const index = loadComponent("../index.tsx");
    expect(index).toMatch(/\/health/);
    expect(index).toMatch(/SystemHealth/);
  });

  it("App.tsx includes a Health nav link", () => {
    const app = loadComponent("../App.tsx");
    expect(app).toMatch(/href="\/health"/);
    expect(app).toMatch(/Health/);
  });
});
