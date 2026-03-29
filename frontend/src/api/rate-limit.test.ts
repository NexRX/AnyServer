import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { resolve } from "path";
import { loadProjectStylesheet } from "../test-utils/loadStylesheet";

function loadFile(relativePath: string): string {
  return readFileSync(resolve(__dirname, "..", relativePath), "utf-8");
}

function loadApiFile(relativePath: string): string {
  return readFileSync(resolve(__dirname, relativePath), "utf-8");
}

// ─── RateLimitError class in core.ts ────────────────────────────────

describe("RateLimitError class exists in api/core.ts", () => {
  const source = loadApiFile("core.ts");

  it("exports a RateLimitError class", () => {
    expect(source).toMatch(/export\s+class\s+RateLimitError\s+extends\s+Error/);
  });

  it("has retryAt and retryAfterSecs public readonly fields", () => {
    expect(source).toMatch(/public\s+readonly\s+retryAt:\s*number/);
    expect(source).toMatch(/public\s+readonly\s+retryAfterSecs:\s*number/);
  });

  it("sets the name property to 'RateLimitError'", () => {
    expect(source).toContain('this.name = "RateLimitError"');
  });

  it("computes retryAt as Date.now() + retryAfterSecs * 1000", () => {
    expect(source).toMatch(
      /this\.retryAt\s*=\s*Date\.now\(\)\s*\+\s*retryAfterSecs\s*\*\s*1000/,
    );
  });
});

// ─── rateLimitRetryAt signal & emitRateLimitEvent in core.ts ────────

describe("rateLimitRetryAt signal and emitRateLimitEvent in api/core.ts", () => {
  const source = loadApiFile("core.ts");

  it("creates a rateLimitRetryAt signal with createSignal", () => {
    expect(source).toMatch(/rateLimitRetryAt/);
    expect(source).toMatch(/createSignal<number\s*\|\s*null>/);
  });

  it("exports rateLimitRetryAt", () => {
    expect(source).toMatch(/export\s*\{\s*rateLimitRetryAt\s*\}/);
  });

  it("exports an emitRateLimitEvent function", () => {
    expect(source).toMatch(
      /export\s+function\s+emitRateLimitEvent\s*\(\s*retryAfterSecs:\s*number\s*\)/,
    );
  });

  it("emitRateLimitEvent sets the signal to Date.now() + retryAfterSecs * 1000", () => {
    const fnBody =
      source.match(/function\s+emitRateLimitEvent[^{]*\{([\s\S]*?)\}/)?.[1] ??
      "";
    expect(fnBody).toMatch(
      /setRateLimitRetryAt\(\s*Date\.now\(\)\s*\+\s*retryAfterSecs\s*\*\s*1000\s*\)/,
    );
  });
});

// ─── 429 intercept in request() ─────────────────────────────────────

describe("request() intercepts 429 responses in api/core.ts", () => {
  const source = loadApiFile("core.ts");

  it("checks for res.status === 429", () => {
    expect(source).toMatch(/res\.status\s*===\s*429/);
  });

  it("parses the retry-after header", () => {
    expect(source).toMatch(/res\.headers\.get\(\s*["']retry-after["']\s*\)/);
  });

  it("calls emitRateLimitEvent on the first 429", () => {
    expect(source).toMatch(/emitRateLimitEvent\(\s*retryAfterSecs\s*\)/);
  });

  it("waits retryAfterSecs * 1000 ms before retrying", () => {
    expect(source).toMatch(
      /setTimeout\s*\(\s*r\s*,\s*retryAfterSecs\s*\*\s*1000\s*\)/,
    );
  });

  it("passes _isRateLimitRetry: true on the retry call", () => {
    expect(source).toContain("_isRateLimitRetry: true");
  });

  it("throws RateLimitError if already a rate-limit retry", () => {
    // Should check _isRateLimitRetry and throw
    expect(source).toMatch(/options\?\._isRateLimitRetry/);
    expect(source).toMatch(
      /throw\s+new\s+RateLimitError\s*\(\s*retryAfterSecs\s*\)/,
    );
  });

  it("accepts _isRateLimitRetry in the options type", () => {
    expect(source).toMatch(/_isRateLimitRetry\?:\s*boolean/);
  });
});

// ─── client.ts barrel exports ───────────────────────────────────────

describe("client.ts barrel exports rate-limit utilities", () => {
  const source = loadApiFile("client.ts");

  it("re-exports RateLimitError", () => {
    expect(source).toMatch(/RateLimitError/);
  });

  it("re-exports rateLimitRetryAt", () => {
    expect(source).toMatch(/rateLimitRetryAt/);
  });

  it("re-exports emitRateLimitEvent", () => {
    expect(source).toMatch(/emitRateLimitEvent/);
  });
});

// ─── RateLimitBanner component ──────────────────────────────────────

describe("RateLimitBanner component", () => {
  const source = loadFile("components/RateLimitBanner.tsx");

  it("imports rateLimitRetryAt from the API core", () => {
    expect(source).toMatch(
      /import\s*\{[^}]*rateLimitRetryAt[^}]*\}\s*from\s*["'][^"']*\/api\/core["']/,
    );
  });

  it("imports createSignal, createEffect, onCleanup, and Show from solid-js", () => {
    expect(source).toMatch(/createSignal/);
    expect(source).toMatch(/createEffect/);
    expect(source).toMatch(/onCleanup/);
    expect(source).toMatch(/Show/);
  });

  it("tracks secondsLeft state", () => {
    expect(source).toMatch(/\bsecondsLeft\b/);
    expect(source).toMatch(/\bsetSecondsLeft\b/);
  });

  it("tracks visible state", () => {
    expect(source).toMatch(/\bvisible\b/);
    expect(source).toMatch(/\bsetVisible\b/);
  });

  it("uses setInterval for ticking and clears it on cleanup", () => {
    expect(source).toMatch(/setInterval\s*\(/);
    expect(source).toMatch(/clearInterval\s*\(/);
    expect(source).toMatch(/onCleanup\s*\(\s*clearTimer\s*\)/);
  });

  it("ticks every 100ms for smooth countdown", () => {
    expect(source).toMatch(/setInterval\s*\(\s*tick\s*,\s*100\s*\)/);
  });

  it("shows 'Resuming…' when countdown reaches zero", () => {
    expect(source).toContain("Resuming");
  });

  it("auto-hides the banner after countdown expires with a short delay", () => {
    expect(source).toMatch(
      /setTimeout\s*\(\s*\(\)\s*=>\s*setVisible\s*\(\s*false\s*\)\s*,\s*800\s*\)/,
    );
  });

  it("renders the rate-limit-banner wrapper with role='alert' and aria-live", () => {
    expect(source).toMatch(/class="rate-limit-banner"/);
    expect(source).toMatch(/role="alert"/);
    expect(source).toMatch(/aria-live="polite"/);
  });

  it("renders a .rate-limit-icon element", () => {
    expect(source).toContain('class="rate-limit-icon"');
  });

  it("renders the countdown value inside a .rate-limit-countdown strong", () => {
    expect(source).toMatch(/class="rate-limit-countdown"/);
    expect(source).toMatch(/\{secondsLeft\(\)\}s/);
  });

  it("renders a progress bar with dynamic width", () => {
    expect(source).toContain('class="rate-limit-progress-track"');
    expect(source).toContain('class="rate-limit-progress-bar"');
    expect(source).toMatch(/style=\{.*width.*progress\(\)/);
  });

  it("computes progress as a percentage based on totalSecs", () => {
    expect(source).toMatch(/\btotalSecs\b/);
    expect(source).toMatch(/progress/);
  });

  it("is exported as default", () => {
    expect(source).toMatch(/export\s+default\s+RateLimitBanner/);
  });
});

// ─── Rate-limit banner CSS styles ───────────────────────────────────

describe("Rate-limit banner CSS styles in styles.css", () => {
  const css = loadProjectStylesheet(__dirname);

  function stripComments(s: string): string {
    return s.replace(/\/\*[\s\S]*?\*\//g, "");
  }

  const stripped = stripComments(css);

  it("defines .rate-limit-banner with fixed positioning and z-index", () => {
    const block = stripped.match(/\.rate-limit-banner\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/position:\s*fixed/);
    expect(block).toMatch(/z-index:\s*9999/);
    expect(block).toMatch(/top:\s*0/);
  });

  it("defines slide-down keyframes animation", () => {
    expect(stripped).toMatch(/@keyframes\s+rateLimitSlideDown/);
    expect(stripped).toMatch(
      /\.rate-limit-banner\s*\{[^}]*animation.*rateLimitSlideDown/,
    );
  });

  it("defines .rate-limit-banner-content with flex centering", () => {
    const block =
      stripped.match(/\.rate-limit-banner-content\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/display:\s*flex/);
    expect(block).toMatch(/align-items:\s*center/);
    expect(block).toMatch(/justify-content:\s*center/);
    expect(block).toMatch(/gap/);
  });

  it("defines .rate-limit-countdown with tabular-nums and mono font", () => {
    const block =
      stripped.match(/\.rate-limit-countdown\s*\{([^}]*)/)?.[1] ?? "";
    expect(block).toMatch(/font-variant-numeric:\s*tabular-nums/);
    expect(block).toMatch(/font-family.*--mono/);
    expect(block).toMatch(/min-width:\s*2ch/);
  });

  it("defines .rate-limit-progress-track and .rate-limit-progress-bar", () => {
    expect(stripped).toMatch(/\.rate-limit-progress-track\s*\{/);
    expect(stripped).toMatch(/\.rate-limit-progress-bar\s*\{/);
    const barBlock =
      stripped.match(/\.rate-limit-progress-bar\s*\{([^}]*)/)?.[1] ?? "";
    expect(barBlock).toMatch(/transition.*width/);
  });

  it("uses --warning color tokens", () => {
    const bannerBlock =
      stripped.match(/\.rate-limit-banner\s*\{([^}]*)/)?.[1] ?? "";
    expect(bannerBlock).toMatch(/--warning/);
  });
});

// ─── RateLimitBanner mounted in App.tsx ─────────────────────────────

describe("RateLimitBanner is mounted in App.tsx", () => {
  const source = loadFile("App.tsx");

  it("imports RateLimitBanner", () => {
    expect(source).toMatch(
      /import\s+RateLimitBanner\s+from\s+["'][^"']*\/RateLimitBanner["']/,
    );
  });

  it("renders <RateLimitBanner /> in the app shell", () => {
    expect(source).toMatch(/<RateLimitBanner[\s\S]*?\/>/);
  });

  it("renders RateLimitBanner at the top level (before navbar)", () => {
    const bannerIdx = source.indexOf("<RateLimitBanner");
    const navIdx = source.indexOf('<nav class="navbar"');
    expect(bannerIdx).toBeGreaterThan(-1);
    expect(navIdx).toBeGreaterThan(-1);
    expect(bannerIdx).toBeLessThan(navIdx);
  });
});

// ─── Login.tsx handles RateLimitError ───────────────────────────────

describe("Login.tsx handles RateLimitError with countdown button", () => {
  const source = loadFile("pages/Login.tsx");

  it("imports RateLimitError", () => {
    expect(source).toMatch(/RateLimitError/);
  });

  it("has a rateLimitCountdown signal", () => {
    expect(source).toMatch(/rateLimitCountdown/);
  });

  it("catches RateLimitError in the login submit handler", () => {
    expect(source).toMatch(/instanceof\s+RateLimitError/);
  });

  it("starts a countdown from retryAfterSecs", () => {
    expect(source).toMatch(
      /startRateLimitCountdown\s*\(\s*e\.retryAfterSecs\s*\)/,
    );
  });

  it("disables the submit button when countdown > 0", () => {
    expect(source).toMatch(/disabled=\{[^}]*rateLimitCountdown\(\)\s*>\s*0/);
  });

  it("shows 'Try again in Ns' label when countdown is active", () => {
    expect(source).toMatch(/Try again in.*rateLimitCountdown\(\)/);
  });

  it("shows rate-limit-specific error message", () => {
    expect(source).toMatch(/Too many attempts/);
  });

  it("also handles RateLimitError in the invite redemption form", () => {
    expect(source).toMatch(/inviteRateLimitCountdown/);
    expect(source).toMatch(/startInviteRateLimitCountdown/);
  });
});

// ─── Register.tsx handles RateLimitError ────────────────────────────

describe("Register.tsx handles RateLimitError with countdown button", () => {
  const source = loadFile("pages/Register.tsx");

  it("imports RateLimitError", () => {
    expect(source).toMatch(/RateLimitError/);
  });

  it("has a rateLimitCountdown signal", () => {
    expect(source).toMatch(/rateLimitCountdown/);
  });

  it("catches RateLimitError in the submit handler", () => {
    expect(source).toMatch(/instanceof\s+RateLimitError/);
  });

  it("disables the submit button when countdown > 0", () => {
    expect(source).toMatch(/disabled=\{[^}]*rateLimitCountdown\(\)\s*>\s*0/);
  });

  it("shows 'Try again in Ns' label when countdown is active", () => {
    expect(source).toMatch(/Try again in.*rateLimitCountdown\(\)/);
  });
});

// ─── Setup.tsx handles RateLimitError ───────────────────────────────

describe("Setup.tsx handles RateLimitError with countdown button", () => {
  const source = loadFile("pages/Setup.tsx");

  it("imports RateLimitError", () => {
    expect(source).toMatch(/RateLimitError/);
  });

  it("has a rateLimitCountdown signal", () => {
    expect(source).toMatch(/rateLimitCountdown/);
  });

  it("catches RateLimitError in the submit handler", () => {
    expect(source).toMatch(/instanceof\s+RateLimitError/);
  });

  it("disables the submit button when countdown > 0", () => {
    expect(source).toMatch(/disabled=\{[^}]*rateLimitCountdown\(\)\s*>\s*0/);
  });

  it("shows 'Try again in Ns' label when countdown is active", () => {
    expect(source).toMatch(/Try again in.*rateLimitCountdown\(\)/);
  });
});

// ─── Auth context uses RateLimitError delay ─────────────────────────

describe("Auth context uses RateLimitError for retry delay", () => {
  const source = loadFile("context/auth.tsx");

  it("imports RateLimitError", () => {
    expect(source).toMatch(/RateLimitError/);
  });

  it("checks for RateLimitError instance in fetchSettingsWithRetry", () => {
    expect(source).toMatch(/err\s+instanceof\s+RateLimitError/);
  });

  it("uses retryAfterSecs * 1000 as delay when rate-limited", () => {
    expect(source).toMatch(/err\.retryAfterSecs\s*\*\s*1000/);
  });

  it("falls back to the fixed backoff for non-rate-limit errors", () => {
    expect(source).toMatch(/delayMs\s*\*\s*attempt/);
  });
});

// ─── Backend rate-limit tier split (api/mod.rs via source inspection) ──

describe("Backend rate-limit tiers are correctly split", () => {
  const source = readFileSync(
    resolve(__dirname, "..", "..", "..", "backend", "src", "api", "mod.rs"),
    "utf-8",
  );

  it("defines a credential rate limit (auth_rate_limit) at 10 req/60s", () => {
    expect(source).toMatch(
      /auth_rate_limit\s*=\s*RateLimitLayer::new\(\s*10\s*,/,
    );
  });

  it("defines a session rate limit at 30 req/60s", () => {
    expect(source).toMatch(
      /session_rate_limit\s*=\s*RateLimitLayer::new\(\s*30\s*,/,
    );
  });

  it("defines a status rate limit at 60 req/60s", () => {
    expect(source).toMatch(
      /status_rate_limit\s*=\s*RateLimitLayer::new\(\s*60\s*,/,
    );
  });

  it("has /auth/login on the credential tier", () => {
    // credential_auth_routes should include /auth/login
    const credBlock =
      source.match(
        /credential_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*auth_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(credBlock).toContain("/auth/login");
  });

  it("has /auth/register on the credential tier", () => {
    const credBlock =
      source.match(
        /credential_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*auth_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(credBlock).toContain("/auth/register");
  });

  it("has /auth/setup on the credential tier", () => {
    const credBlock =
      source.match(
        /credential_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*auth_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(credBlock).toContain("/auth/setup");
  });

  it("has /auth/redeem-invite on its own invite rate-limit tier", () => {
    const inviteBlock =
      source.match(
        /invite_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*invite_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(inviteBlock).toContain("/auth/redeem-invite");
  });

  it("defines an invite rate limit at 3 req/300s", () => {
    expect(source).toMatch(
      /invite_rate_limit\s*=\s*RateLimitLayer::new\(\s*3\s*,/,
    );
  });

  it("has /auth/refresh on the session tier", () => {
    const sessionBlock =
      source.match(
        /session_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*session_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(sessionBlock).toContain("/auth/refresh");
  });

  it("has /auth/logout on the session tier", () => {
    const sessionBlock =
      source.match(
        /session_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*session_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(sessionBlock).toContain("/auth/logout");
  });

  it("has /auth/status on the status tier", () => {
    const statusBlock =
      source.match(
        /status_auth_routes\s*=\s*Router::new\(\)([\s\S]*?)\.layer\s*\(\s*status_rate_limit\s*\)/,
      )?.[1] ?? "";
    expect(statusBlock).toContain("/auth/status");
  });

  it("merges all four route groups into the final router", () => {
    expect(source).toMatch(/credential_auth_routes/);
    expect(source).toMatch(/\.merge\s*\(\s*invite_auth_routes\s*\)/);
    expect(source).toMatch(/\.merge\s*\(\s*session_auth_routes\s*\)/);
    expect(source).toMatch(/\.merge\s*\(\s*status_auth_routes\s*\)/);
  });

  it("spawns eviction tasks for all rate-limit states", () => {
    // Count spawn_eviction_task calls — should include auth, session,
    // status, invite, plus the other existing ones
    const evictionCalls = (source.match(/spawn_eviction_task\s*\(/g) || [])
      .length;
    expect(evictionCalls).toBeGreaterThanOrEqual(8);
  });

  it("does NOT have a single public_auth_routes grouping anymore", () => {
    expect(source).not.toMatch(/public_auth_routes\s*=/);
  });
});
