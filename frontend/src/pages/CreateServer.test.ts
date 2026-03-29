import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

// ─── Ticket 0-003: "Change Template" button not squished ────────────

describe("Change Template button has anti-squish styles (ticket 0-003)", () => {
  const source = loadComponent("CreateServer.tsx");

  // Extract the button block that contains "Change Template"
  const buttonBlocks = source.match(/<button[\s\S]*?<\/button>/g) || [];
  const changeTemplateButton =
    buttonBlocks.find((b) => b.includes("Change Template")) ?? "";

  it("finds the Change Template button in the source", () => {
    expect(changeTemplateButton.length).toBeGreaterThan(0);
  });

  it("has white-space: nowrap to prevent text wrapping", () => {
    expect(changeTemplateButton).toMatch(/["']white-space["']\s*:\s*["']nowrap["']/);
  });

  it("has padding-inline for horizontal breathing room", () => {
    expect(changeTemplateButton).toMatch(/["']padding-inline["']\s*:\s*["']1rem["']/);
  });

  it("has flex-shrink: 0 to prevent the button from being compressed", () => {
    expect(changeTemplateButton).toMatch(/["']flex-shrink["']\s*:\s*["']0["']/);
  });

  it("still uses btn btn-sm classes", () => {
    expect(changeTemplateButton).toMatch(/class=["']btn btn-sm["']/);
  });

  it("applies styles via an inline style object (not a CSS class)", () => {
    expect(changeTemplateButton).toMatch(/style=\{/);
  });
});
