import { describe, it, expect } from "vitest";
import { readFileSync, existsSync, readdirSync } from "fs";
import { resolve } from "path";

const ROOT = resolve(__dirname, "..");

function loadJson(relativePath: string): unknown {
  return JSON.parse(readFileSync(resolve(ROOT, relativePath), "utf-8"));
}

function loadText(relativePath: string): string {
  return readFileSync(resolve(ROOT, relativePath), "utf-8");
}

// ─── tsconfig.test.json structure ───────────────────────────────────

describe("tsconfig.test.json", () => {
  const path = resolve(ROOT, "tsconfig.test.json");

  it("exists and extends the base tsconfig", () => {
    expect(existsSync(path)).toBe(true);
    const config = loadJson("tsconfig.test.json") as Record<string, unknown>;
    expect(config.extends).toBe("./tsconfig.json");
  });

  it("includes node and vite/client in compilerOptions.types", () => {
    const config = loadJson("tsconfig.test.json") as {
      compilerOptions?: { types?: string[] };
    };
    expect(config.compilerOptions?.types).toContain("node");
    expect(config.compilerOptions!.types).toContain("vite/client");
  });

  it("scopes include to test and spec files only", () => {
    const config = loadJson("tsconfig.test.json") as { include?: string[] };
    expect(config.include).toBeDefined();
    expect(config.include!.some((g) => g.includes("*.test.ts"))).toBe(true);
    expect(config.include!.some((g) => g.includes("*.spec.ts"))).toBe(true);
    expect(config.include).not.toContain("src");
  });
});

describe("base tsconfig.json includes test types for IDE coverage", () => {
  it("has vite/client, node, and jest-dom/vitest in types", () => {
    const config = loadJson("tsconfig.json") as {
      compilerOptions?: { types?: string[] };
      include?: string[];
    };
    const types = config.compilerOptions!.types!;
    expect(types).toContain("vite/client");
    expect(types).toContain("node");
    expect(types).toContain("@testing-library/jest-dom/vitest");
    expect(config.include).toContain("src");
  });
});

// ─── Vitest config excludes Playwright e2e tests ────────────────────

describe("vite.config.ts test configuration", () => {
  const configSource = loadText("vite.config.ts");

  it("excludes e2e/** and node_modules/** from Vitest", () => {
    expect(configSource).toMatch(/exclude\s*:\s*\[[\s\S]*["']e2e\/\*\*["']/);
    expect(configSource).toMatch(
      /exclude\s*:\s*\[[\s\S]*["']node_modules\/\*\*["']/,
    );
  });

  it("references tsconfig.test.json for typecheck", () => {
    expect(configSource).toMatch(
      /typecheck\s*:\s*\{[\s\S]*tsconfig\s*:\s*["']\.\/tsconfig\.test\.json["']/,
    );
  });
});

describe("Playwright and Vitest are properly separated", () => {
  it("package.json has separate test (vitest) and test:e2e (playwright) scripts", () => {
    const pkg = loadJson("package.json") as {
      scripts?: Record<string, string>;
    };
    expect(pkg.scripts!["test"]).toMatch(/vitest/i);
    expect(pkg.scripts!["test"]).not.toMatch(/playwright/i);
    expect(pkg.scripts!["test:e2e"]).toMatch(/playwright/i);
  });

  it("e2e spec files import from Playwright, not Vitest", () => {
    const specFiles = readdirSync(resolve(ROOT, "e2e", "tests")).filter((f) =>
      f.endsWith(".spec.ts"),
    );
    expect(specFiles.length).toBeGreaterThan(0);
    for (const file of specFiles) {
      const content = readFileSync(
        resolve(ROOT, "e2e", "tests", file),
        "utf-8",
      );
      expect(content).toMatch(
        /from\s+["']@playwright\/test["']|from\s+["']\.\.\/fixtures\/test-environment["']/,
      );
      expect(content).not.toMatch(
        /from\s+["']vitest["']|require\s*\(\s*["']vitest["']\s*\)/,
      );
    }
  });
});

// ─── @types/node availability ───────────────────────────────────────

describe("@types/node dev dependency", () => {
  it("is listed in package.json and installed", () => {
    const pkg = loadJson("package.json") as {
      devDependencies?: Record<string, string>;
    };
    expect(pkg.devDependencies).toHaveProperty("@types/node");
    expect(existsSync(resolve(ROOT, "node_modules", "@types", "node"))).toBe(
      true,
    );
  });

  it("is exercised by test files importing fs or path", () => {
    const testFiles = [
      "src/App.test.ts",
      "src/config.test.ts",
      "src/styles.test.ts",
      "src/components/Loader.test.ts",
    ];
    let usesNodeBuiltin = false;
    let usesDirname = false;
    for (const file of testFiles) {
      const content = loadText(file);
      if (
        content.includes('from "fs"') ||
        content.includes('from "path"') ||
        content.includes("from 'fs'") ||
        content.includes("from 'path'")
      ) {
        usesNodeBuiltin = true;
      }
      if (content.includes("__dirname")) {
        usesDirname = true;
      }
    }
    expect(usesNodeBuiltin).toBe(true);
    expect(usesDirname).toBe(true);
  });
});
