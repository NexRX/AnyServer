import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

// ─── Helpers ────────────────────────────────────────────────────────

function loadComponent(relativePath: string): string {
  const fullPath = resolve(__dirname, relativePath);
  return readFileSync(fullPath, "utf-8");
}

// ═══════════════════════════════════════════════════════════════════
// WizardStartStep runtime helper toggles
// ═══════════════════════════════════════════════════════════════════

describe("WizardStartStep runtime helper toggles", () => {
  const source = loadComponent("./WizardStartStep.tsx");

  it("imports createMemo for computed helper visibility", () => {
    expect(source).toMatch(/createMemo/);
  });

  it("creates showJavaHelper memo that checks binary or manual toggle", () => {
    expect(source).toMatch(/showJavaHelper/);
    expect(source).toMatch(/isJavaBinary.*enable_java_helper/);
  });

  it("creates showDotnetHelper memo that checks binary or manual toggle", () => {
    expect(source).toMatch(/showDotnetHelper/);
    expect(source).toMatch(/isDotnetBinary.*enable_dotnet_helper/);
  });

  it("uses showJavaHelper instead of isJavaBinary for JavaRuntimeSelector visibility", () => {
    expect(source).toMatch(/<Show\s+when=\{showJavaHelper\(\)\}/);
  });

  it("uses showDotnetHelper instead of isDotnetBinary for DotnetRuntimeSelector visibility", () => {
    expect(source).toMatch(/<Show\s+when=\{showDotnetHelper\(\)\}/);
  });

  it("renders a Runtime Helpers heading", () => {
    expect(source).toMatch(/<h4[^>]*>Runtime Helpers<\/h4>/);
  });

  it("renders checkbox for enabling Java runtime helper", () => {
    expect(source).toMatch(/Enable Java Runtime Helper/);
    expect(source).toMatch(/enable_java_helper/);
  });

  it("renders checkbox for enabling .NET runtime helper", () => {
    expect(source).toMatch(/Enable \.NET Runtime Helper/);
    expect(source).toMatch(/enable_dotnet_helper/);
  });

  it("shows tip box when either helper is enabled", () => {
    expect(source).toMatch(/enable_java_helper.*enable_dotnet_helper/s);
    expect(source).toMatch(/Tip:/);
    expect(source).toMatch(/custom wrapper/i);
  });

  it("checkbox updates enable_java_helper via onPatchConfig", () => {
    expect(source).toMatch(/props\.config\.enable_java_helper/);
    expect(source).toMatch(/onPatchConfig.*enable_java_helper.*checked/s);
  });

  it("checkbox updates enable_dotnet_helper via onPatchConfig", () => {
    expect(source).toMatch(/props\.config\.enable_dotnet_helper/);
    expect(source).toMatch(/onPatchConfig.*enable_dotnet_helper.*checked/s);
  });

  it("explains that toggles work for custom binaries", () => {
    expect(source).toMatch(/doesn't match auto-detection patterns/i);
  });

  it("mentions use case for wrapper scripts and launchers", () => {
    expect(source).toMatch(/custom wrapper/i);
    expect(source).toMatch(/launchers/i);
  });
});

// ═══════════════════════════════════════════════════════════════════
// WizardStartStep helper selector integration
// ═══════════════════════════════════════════════════════════════════

describe("WizardStartStep helper selector integration", () => {
  const source = loadComponent("./WizardStartStep.tsx");

  it("imports isJavaBinary helper function", () => {
    expect(source).toMatch(/import.*isJavaBinary.*from.*JavaRuntimeSelector/);
  });

  it("imports isDotnetBinary helper function", () => {
    expect(source).toMatch(/isDotnetBinary/);
  });

  it("imports JavaRuntimeSelector component", () => {
    expect(source).toMatch(/import.*JavaRuntimeSelector/);
  });

  it("imports DotnetRuntimeSelector component", () => {
    expect(source).toMatch(/import.*DotnetRuntimeSelector/);
  });

  it("passes currentBinary prop to JavaRuntimeSelector", () => {
    expect(source).toMatch(
      /<JavaRuntimeSelector[\s\S]*?currentBinary=\{props\.config\.binary\}/,
    );
  });

  it("passes currentEnv prop to JavaRuntimeSelector", () => {
    expect(source).toMatch(
      /<JavaRuntimeSelector[\s\S]*?currentEnv=\{props\.config\.env\}/,
    );
  });

  it("passes currentBinary and currentEnv props to DotnetRuntimeSelector", () => {
    expect(source).toMatch(
      /<DotnetRuntimeSelector[\s\S]*?currentBinary=\{props\.config\.binary\}/,
    );
    expect(source).toMatch(
      /<DotnetRuntimeSelector[\s\S]*?currentEnv=\{props\.config\.env\}/,
    );
  });

  it("JavaRuntimeSelector onEnvChange merges env vars and updates via onPatchConfig", () => {
    expect(source).toMatch(
      /<JavaRuntimeSelector[\s\S]*?onEnvChange=\{.*envVars[\s\S]*?merged[\s\S]*?onPatchConfig.*env.*merged/s,
    );
  });

  it("DotnetRuntimeSelector onSelect merges env vars and updates via onPatchConfig", () => {
    expect(source).toMatch(
      /<DotnetRuntimeSelector[\s\S]*?onSelect=\{.*envVars[\s\S]*?merged[\s\S]*?onPatchConfig.*env.*merged/s,
    );
  });
});

// ═══════════════════════════════════════════════════════════════════
// Wizard types default config
// ═══════════════════════════════════════════════════════════════════

describe("Wizard types defaultConfig includes helper flags", () => {
  const source = loadComponent("./types.ts");

  it("includes enable_java_helper in defaultConfig", () => {
    expect(source).toMatch(/enable_java_helper:\s*(false|true)/);
  });

  it("includes enable_dotnet_helper in defaultConfig", () => {
    expect(source).toMatch(/enable_dotnet_helper:\s*(false|true)/);
  });

  it("defaults enable_java_helper to false", () => {
    expect(source).toMatch(/enable_java_helper:\s*false/);
  });

  it("defaults enable_dotnet_helper to false", () => {
    expect(source).toMatch(/enable_dotnet_helper:\s*false/);
  });
});
