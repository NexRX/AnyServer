import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";

function loadComponent(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

// ─── Ticket 0-004: Invite code form replaces login form (single card) ──

describe("Login page shows only one card at a time (ticket 0-004)", () => {
  const source = loadComponent("Login.tsx");

  it("uses <Show when={!showInvite()} fallback={...}> to toggle between cards", () => {
    // The Show component should use !showInvite() as the condition
    // and render the invite card as the fallback
    expect(source).toMatch(
      /<Show[\s\S]*?when=\{!showInvite\(\)\}[\s\S]*?fallback=/,
    );
  });

  it("does NOT render both cards simultaneously with a separate <Show when={showInvite()}>", () => {
    // The old pattern was: <Show when={showInvite()}><div class="auth-card" style={{ "margin-top": ...
    // This should no longer exist as a standalone Show block appending a second card
    expect(source).not.toMatch(
      /<Show\s+when=\{showInvite\(\)\}>\s*<div\s+class="auth-card"\s+style=/,
    );
  });

  it("has exactly one top-level auth-page div containing a single Show toggle", () => {
    // The auth-page should contain a single <Show> that switches between
    // login card and invite card — not two separate card divs
    const authPageBlock =
      source.match(
        /<div\s+class="auth-page">([\s\S]*?)\n\s{4}<\/div>\s*\n\s{2}\);\s*\n/,
      )?.[1] ?? "";
    // Count direct auth-card divs (there should be exactly two: one in
    // the fallback and one in the main branch, but NOT side-by-side)
    const authCardCount = (authPageBlock.match(/class="auth-card"/g) || [])
      .length;
    expect(authCardCount).toBe(2); // one in fallback, one in main branch
  });

  it("renders the login form inside the main branch (not fallback)", () => {
    // The login form should be in the "when" branch (shown when !showInvite)
    expect(source).toMatch(
      /when=\{!showInvite\(\)\}[\s\S]*?Sign in to your account/,
    );
  });

  it("renders the invite form inside the fallback branch", () => {
    expect(source).toMatch(/fallback=\{[\s\S]*?Redeem Invite Code/);
  });
});

describe("Login page clears errors when toggling between forms (ticket 0-004)", () => {
  const source = loadComponent("Login.tsx");

  it("defines a switchToInvite function that clears login error", () => {
    const fnBody =
      source.match(
        /const\s+switchToInvite\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\};/,
      )?.[1] ?? "";
    expect(fnBody).toMatch(/setError\(\s*null\s*\)/);
    expect(fnBody).toMatch(/setShowInvite\(\s*true\s*\)/);
  });

  it("defines a switchToLogin function that clears invite error and success", () => {
    const fnBody =
      source.match(
        /const\s+switchToLogin\s*=\s*\(\)\s*=>\s*\{([\s\S]*?)\};/,
      )?.[1] ?? "";
    expect(fnBody).toMatch(/setInviteError\(\s*null\s*\)/);
    expect(fnBody).toMatch(/setInviteSuccess\(\s*false\s*\)/);
    expect(fnBody).toMatch(/setShowInvite\(\s*false\s*\)/);
  });

  it("uses switchToInvite (not raw setShowInvite) for the 'Have an invite code?' button", () => {
    // Find the button that says "Have an invite code?" and verify it calls switchToInvite
    const inviteButtonBlock =
      source.match(
        /<button[\s\S]*?Have an invite code\?[\s\S]*?<\/button>/,
      )?.[0] ?? "";
    expect(inviteButtonBlock).toMatch(/onClick=\{switchToInvite\}/);
    expect(inviteButtonBlock).not.toMatch(/onClick=\{.*setShowInvite/);
  });

  it("uses switchToLogin (not raw setShowInvite) for the 'Back to login' button", () => {
    const backButtonBlock =
      source.match(/<button[\s\S]*?Back to login[\s\S]*?<\/button>/)?.[0] ?? "";
    expect(backButtonBlock).toMatch(/onClick=\{switchToLogin\}/);
    expect(backButtonBlock).not.toMatch(/onClick=\{.*setShowInvite/);
  });
});

describe("Login page invite card has its own footer with Back button (ticket 0-004)", () => {
  const source = loadComponent("Login.tsx");

  it("has a 'Back to login' button inside the invite card's auth-footer", () => {
    // The fallback branch should contain its own auth-footer with the back button
    const fallbackBlock =
      source.match(/fallback=\{([\s\S]*?)\}\s*>\s*\{\/\*.*Login Card/)?.[1] ??
      "";
    expect(fallbackBlock).toContain("auth-footer");
    expect(fallbackBlock).toContain("Back to login");
  });

  it("the 'Have an invite code?' button is inside the login card's auth-footer", () => {
    // After the fallback closes, the login card section runs from the
    // "Login Card" comment to the outer </Show> that closes the toggle.
    // Use a greedy match so we capture past inner </Show> tags.
    const loginCardBlock =
      source.match(
        /\{\/\*.*Login Card.*\*\/\}([\s\S]*)<\/Show>\s*\n\s*<\/div>\s*\n\s*\);/,
      )?.[1] ?? "";
    expect(loginCardBlock).toContain("auth-footer");
    expect(loginCardBlock).toContain("Have an invite code?");
  });

  it("the login card does NOT contain 'Back to login'", () => {
    const loginCardBlock =
      source.match(
        /\{\/\*.*Login Card.*\*\/\}([\s\S]*)<\/Show>\s*\n\s*<\/div>\s*\n\s*\);/,
      )?.[1] ?? "";
    expect(loginCardBlock).not.toContain("Back to login");
  });

  it("the invite card does NOT contain 'Have an invite code?'", () => {
    const fallbackBlock =
      source.match(/fallback=\{([\s\S]*?)\}\s*>\s*\{\/\*.*Login Card/)?.[1] ??
      "";
    expect(fallbackBlock).not.toContain("Have an invite code?");
  });
});

describe("Login page preserves existing behaviour (ticket 0-004)", () => {
  const source = loadComponent("Login.tsx");

  it("still handles RateLimitError for login form", () => {
    expect(source).toMatch(/instanceof\s+RateLimitError/);
    expect(source).toMatch(/startRateLimitCountdown/);
  });

  it("still handles RateLimitError for invite form", () => {
    expect(source).toMatch(/startInviteRateLimitCountdown/);
    expect(source).toMatch(/inviteRateLimitCountdown/);
  });

  it("still shows registration link when enabled", () => {
    expect(source).toMatch(/isRegistrationEnabled/);
    expect(source).toContain("Create one");
  });

  it("still shows setup link when setup is incomplete", () => {
    expect(source).toMatch(/isSetupComplete/);
    expect(source).toContain("Set up AnyServer");
  });

  it("the invite toggle only appears when setup is complete", () => {
    // The "Have an invite code?" button should be inside a Show when={auth.isSetupComplete()}
    // Extract the login card section (greedy to pass inner </Show> tags)
    const loginCardBlock =
      source.match(
        /\{\/\*.*Login Card.*\*\/\}([\s\S]*)<\/Show>\s*\n\s*<\/div>\s*\n\s*\);/,
      )?.[1] ?? "";
    expect(loginCardBlock).toMatch(/auth\.isSetupComplete\(\)/);
    // The invite button should appear after isSetupComplete check
    expect(loginCardBlock).toContain("Have an invite code?");
  });
});
