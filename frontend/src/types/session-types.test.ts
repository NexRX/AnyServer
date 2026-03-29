import { describe, it, expect } from "vitest";
import type {
  SessionListResponse,
  RevokeSessionRequest,
  RevokeSessionResponse,
  SessionInfo,
} from "./bindings";

// ─── Ticket 053: Missing Session Management TypeScript Types ────────

describe("Session management types are exported from bindings (ticket 053)", () => {
  it("SessionListResponse type is exported and accessible", () => {
    // Type-only test — if this compiles, the type exists
    const _typeCheck: SessionListResponse = {
      sessions: [],
    };
    expect(true).toBe(true);
  });

  it("RevokeSessionRequest type is exported and accessible", () => {
    // Type-only test — if this compiles, the type exists
    const _typeCheck: RevokeSessionRequest = {
      family_id: "test-family-id",
    };
    expect(true).toBe(true);
  });

  it("RevokeSessionResponse type is exported and accessible", () => {
    // Type-only test — if this compiles, the type exists
    const _typeCheck: RevokeSessionResponse = {
      revoked_count: 1n,
    };
    expect(true).toBe(true);
  });

  it("SessionInfo type is exported and accessible", () => {
    // Type-only test — if this compiles, the type exists
    const _typeCheck: SessionInfo = {
      id: "test-id",
      family_id: "test-family-id",
      created_at: "2026-04-01T00:00:00Z",
      expires_at: "2026-04-01T01:00:00Z",
      is_current: false,
    };
    expect(true).toBe(true);
  });
});

describe("Session types have correct structure (ticket 053)", () => {
  it("SessionListResponse contains sessions array", () => {
    const response: SessionListResponse = {
      sessions: [
        {
          id: "session-1",
          family_id: "family-1",
          created_at: "2026-04-01T00:00:00Z",
          expires_at: "2026-04-01T01:00:00Z",
          is_current: true,
        },
        {
          id: "session-2",
          family_id: "family-2",
          created_at: "2026-04-01T00:00:00Z",
          expires_at: "2026-04-01T01:00:00Z",
          is_current: false,
        },
      ],
    };
    expect(response.sessions).toHaveLength(2);
    expect(response.sessions[0].is_current).toBe(true);
  });

  it("RevokeSessionRequest requires family_id", () => {
    const request: RevokeSessionRequest = {
      family_id: "test-family-id",
    };
    expect(request.family_id).toBe("test-family-id");
  });

  it("RevokeSessionResponse contains revoked_count", () => {
    const response: RevokeSessionResponse = {
      revoked_count: 2n,
    };
    expect(response.revoked_count).toBe(2n);
  });

  it("SessionInfo has all required fields", () => {
    const session: SessionInfo = {
      id: "test-id",
      family_id: "test-family",
      created_at: "2026-04-01T00:00:00Z",
      expires_at: "2026-04-01T01:00:00Z",
      is_current: false,
    };
    expect(session.id).toBeDefined();
    expect(session.family_id).toBeDefined();
    expect(session.created_at).toBeDefined();
    expect(session.expires_at).toBeDefined();
    expect(typeof session.is_current).toBe("boolean");
  });
});

describe("Session types can be used with auth API functions (ticket 053)", () => {
  it("auth.ts imports compile without errors", async () => {
    // This test verifies that the auth.ts file can be imported
    // without TypeScript compilation errors
    const authModule = await import("../api/auth");
    expect(authModule.listSessions).toBeDefined();
    expect(authModule.revokeSession).toBeDefined();
    expect(typeof authModule.listSessions).toBe("function");
    expect(typeof authModule.revokeSession).toBe("function");
  });
});
