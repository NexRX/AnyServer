import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

// ─── Ticket 001: Remove Ctrl+C Option from Console View ────────────

describe("Ctrl+C button removed from ServerDetail (ticket 001)", () => {
  const source = loadComponent("ServerDetail.tsx");

  it("does not contain any Ctrl+C / sigint references", () => {
    expect(source).not.toMatch(/>\s*Ctrl\+C\s*<\/button>/);
    expect(source).not.toMatch(/handleSigint/);
    expect(source).not.toMatch(/sendSigint/);
    expect(source).not.toMatch(/title="Send SIGINT/);
    expect(source).not.toMatch(/"\^C"/);
  });

  it("still has Stop, Kill, Start, and Restart buttons with handlers", () => {
    for (const btn of ["Stop", "Kill", "Start", "Restart"]) {
      expect(source, `missing ${btn} button`).toMatch(
        new RegExp(`${btn}\\s*\\n\\s*<\\/button>`),
      );
    }
    expect(source).toMatch(/handleStop/);
    expect(source).toMatch(/handleKill/);
    expect(source).toMatch(/stopServer/);
    expect(source).toMatch(/killServer/);
  });
});

// ─── Backend sigint endpoint preserved for programmatic use ─────────

describe("sendSigint API function preserved (ticket 001)", () => {
  it("api/servers.ts exports sendSigint calling /sigint endpoint", () => {
    const apiSource = loadComponent("../api/servers.ts");
    expect(apiSource).toMatch(/export function sendSigint/);
    expect(apiSource).toMatch(/\/sigint/);
  });

  it("api/client.ts re-exports sendSigint", () => {
    const clientSource = loadComponent("../api/client.ts");
    expect(clientSource).toMatch(/sendSigint/);
  });
});
