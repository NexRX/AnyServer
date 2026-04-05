import { describe, it, expect } from "vitest";
import {
  formatBytes,
  formatUptime,
  effectiveElapsed,
  computeShutdownRemaining,
  computeShutdownPercent,
  computeGraceRemaining,
  computeGracePercent,
  formatShutdownCountdown,
} from "./format";

// ─── formatBytes ────────────────────────────────────────────────────

describe("formatBytes", () => {
  it("returns '0 B' for zero bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("formats bytes below 1 KB", () => {
    expect(formatBytes(512)).toBe("512 B");
  });

  it("formats exactly 1 KB", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
  });

  it("formats kilobytes with one decimal", () => {
    expect(formatBytes(1536)).toBe("1.5 KB");
  });

  it("formats megabytes", () => {
    expect(formatBytes(1048576)).toBe("1.0 MB");
  });

  it("formats megabytes with fractional part", () => {
    // 3.5 MB = 3.5 * 1024 * 1024 = 3670016
    expect(formatBytes(3670016)).toBe("3.5 MB");
  });

  it("formats gigabytes", () => {
    expect(formatBytes(1073741824)).toBe("1.0 GB");
  });

  it("formats terabytes", () => {
    // 2 TB = 2 * 1024^4
    expect(formatBytes(2 * Math.pow(1024, 4))).toBe("2.0 TB");
  });

  it("formats petabytes", () => {
    expect(formatBytes(Math.pow(1024, 5))).toBe("1.0 PB");
  });

  it("formats large gigabyte values correctly", () => {
    // 5 GB file — previously would overflow a u32 in the backend
    const fiveGB = 5 * 1024 * 1024 * 1024;
    expect(formatBytes(fiveGB)).toBe("5.0 GB");
  });

  it("formats 1 byte correctly (no decimal)", () => {
    expect(formatBytes(1)).toBe("1 B");
  });

  it("handles values just under a unit boundary", () => {
    // 1023 bytes should still be in bytes
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("handles values just over a unit boundary", () => {
    expect(formatBytes(1025)).toBe("1.0 KB");
  });
});

// ─── formatUptime ───────────────────────────────────────────────────

describe("formatUptime", () => {
  it("formats zero seconds", () => {
    expect(formatUptime(0)).toBe("0s");
  });

  it("formats seconds only", () => {
    expect(formatUptime(45)).toBe("45s");
  });

  it("formats minutes and seconds", () => {
    expect(formatUptime(125)).toBe("2m 5s");
  });

  it("formats hours, minutes, and seconds", () => {
    // 1h 30m 15s = 3600 + 1800 + 15 = 5415
    expect(formatUptime(5415)).toBe("1h 30m 15s");
  });

  it("formats days, hours, minutes, and seconds", () => {
    // 2d 5h 13m 7s = 2*86400 + 5*3600 + 13*60 + 7 = 172800 + 18000 + 780 + 7 = 191587
    expect(formatUptime(191587)).toBe("2d 5h 13m 7s");
  });

  it("omits zero days", () => {
    expect(formatUptime(3661)).toBe("1h 1m 1s");
  });

  it("omits zero hours when days are present", () => {
    // 1d 0h 5m 0s = 86400 + 300 = 86700
    expect(formatUptime(86700)).toBe("1d 5m 0s");
  });

  it("omits zero minutes when only days and seconds", () => {
    // 1d 0h 0m 30s = 86430
    expect(formatUptime(86430)).toBe("1d 30s");
  });

  it("formats exactly one day", () => {
    expect(formatUptime(86400)).toBe("1d 0s");
  });

  it("formats exactly one hour", () => {
    expect(formatUptime(3600)).toBe("1h 0s");
  });

  it("formats exactly one minute", () => {
    expect(formatUptime(60)).toBe("1m 0s");
  });

  it("always includes seconds", () => {
    // Even when seconds are 0, the 's' part is always present
    expect(formatUptime(3600)).toContain("s");
  });

  it("handles fractional seconds by flooring", () => {
    expect(formatUptime(65.9)).toBe("1m 5s");
  });

  it("handles large uptimes", () => {
    // 365 days
    const oneYear = 365 * 86400;
    expect(formatUptime(oneYear)).toBe("365d 0s");
  });
});

// ─── effectiveElapsed ───────────────────────────────────────────────

describe("effectiveElapsed", () => {
  it("returns elapsedSecs when no local time has passed", () => {
    const received = 1000000;
    expect(effectiveElapsed(5, received, received)).toBe(5);
  });

  it("adds local time delta to elapsed_secs", () => {
    const received = 1000000;
    const now = received + 3000; // 3 seconds later
    expect(effectiveElapsed(5, received, now)).toBe(8);
  });

  it("handles fractional local deltas", () => {
    const received = 1000000;
    const now = received + 1500; // 1.5 seconds later
    expect(effectiveElapsed(10, received, now)).toBe(11.5);
  });

  it("clamps negative local delta to zero", () => {
    // If the clock went backwards somehow, don't subtract
    const received = 1000000;
    const now = received - 500;
    expect(effectiveElapsed(5, received, now)).toBe(5);
  });
});

// ─── computeShutdownRemaining ───────────────────────────────────────

describe("computeShutdownRemaining", () => {
  it("returns full timeout when nothing has elapsed", () => {
    const now = 1000000;
    expect(computeShutdownRemaining(0, 30, now, now)).toBe(30);
  });

  it("decreases as backend elapsed_secs increases", () => {
    const now = 1000000;
    expect(computeShutdownRemaining(10, 30, now, now)).toBe(20);
  });

  it("decreases as local time passes between WS messages", () => {
    const received = 1000000;
    const now = received + 5000; // 5 seconds later locally
    // Backend said 10s elapsed out of 30s, plus 5s local = 15s elapsed → 15s remaining
    expect(computeShutdownRemaining(10, 30, received, now)).toBe(15);
  });

  it("never goes below zero", () => {
    const received = 1000000;
    const now = received + 60000; // way past the timeout
    expect(computeShutdownRemaining(10, 30, received, now)).toBe(0);
  });

  it("returns zero when elapsed equals timeout", () => {
    const now = 1000000;
    expect(computeShutdownRemaining(30, 30, now, now)).toBe(0);
  });

  it("counts down second by second with 1s local ticks", () => {
    const received = 1000000;
    // Simulate backend saying 0s elapsed, 10s timeout
    // Then local ticks at 1s, 2s, 3s...
    expect(computeShutdownRemaining(0, 10, received, received + 1000)).toBe(9);
    expect(computeShutdownRemaining(0, 10, received, received + 2000)).toBe(8);
    expect(computeShutdownRemaining(0, 10, received, received + 3000)).toBe(7);
    expect(computeShutdownRemaining(0, 10, received, received + 9000)).toBe(1);
    expect(computeShutdownRemaining(0, 10, received, received + 10000)).toBe(0);
    expect(computeShutdownRemaining(0, 10, received, received + 11000)).toBe(0);
  });

  it("picks up from a mid-shutdown WS update and keeps counting", () => {
    // Backend sends update at elapsed=12s, timeout=30s
    const received = 2000000;
    // 0s local → 18s remaining
    expect(computeShutdownRemaining(12, 30, received, received)).toBe(18);
    // 5s local → 13s remaining
    expect(computeShutdownRemaining(12, 30, received, received + 5000)).toBe(
      13,
    );
    // 18s local → 0s remaining
    expect(computeShutdownRemaining(12, 30, received, received + 18000)).toBe(
      0,
    );
  });
});

// ─── computeShutdownPercent ─────────────────────────────────────────

describe("computeShutdownPercent", () => {
  it("returns 0 when nothing has elapsed", () => {
    const now = 1000000;
    expect(computeShutdownPercent(0, 30, now, now)).toBe(0);
  });

  it("returns 50 when half the timeout has elapsed", () => {
    const now = 1000000;
    expect(computeShutdownPercent(15, 30, now, now)).toBe(50);
  });

  it("increases as local time passes", () => {
    const received = 1000000;
    // 0s elapsed, 20s timeout, 5s local → 25%
    expect(computeShutdownPercent(0, 20, received, received + 5000)).toBe(25);
  });

  it("clamps to 100 when past the timeout", () => {
    const received = 1000000;
    expect(computeShutdownPercent(0, 10, received, received + 20000)).toBe(100);
  });

  it("returns 100 when timeout is zero", () => {
    const now = 1000000;
    expect(computeShutdownPercent(0, 0, now, now)).toBe(100);
  });

  it("returns 100 when timeout is negative", () => {
    const now = 1000000;
    expect(computeShutdownPercent(0, -1, now, now)).toBe(100);
  });

  it("progresses second by second", () => {
    const received = 1000000;
    // 0s elapsed, 10s timeout
    expect(computeShutdownPercent(0, 10, received, received + 1000)).toBe(10);
    expect(computeShutdownPercent(0, 10, received, received + 5000)).toBe(50);
    expect(computeShutdownPercent(0, 10, received, received + 10000)).toBe(100);
  });
});

// ─── formatShutdownCountdown ────────────────────────────────────────

describe("formatShutdownCountdown", () => {
  it("formats zero seconds", () => {
    expect(formatShutdownCountdown(0)).toBe("0s");
  });

  it("formats whole seconds below a minute", () => {
    expect(formatShutdownCountdown(15)).toBe("15s");
  });

  it("rounds fractional seconds up (ceil)", () => {
    expect(formatShutdownCountdown(14.1)).toBe("15s");
    expect(formatShutdownCountdown(0.3)).toBe("1s");
  });

  it("formats exactly 60 seconds as 1m 0s", () => {
    expect(formatShutdownCountdown(60)).toBe("1m 0s");
  });

  it("formats minutes and seconds", () => {
    expect(formatShutdownCountdown(90)).toBe("1m 30s");
    expect(formatShutdownCountdown(135)).toBe("2m 15s");
  });

  it("formats large values", () => {
    // 5m 0s = 300s
    expect(formatShutdownCountdown(300)).toBe("5m 0s");
  });

  it("rounds fractional seconds in the minutes range", () => {
    // 89.2s → ceil = 90s → 1m 30s
    expect(formatShutdownCountdown(89.2)).toBe("1m 30s");
  });

  it("treats negative values as 0s", () => {
    expect(formatShutdownCountdown(-5)).toBe("0s");
  });
});

// ─── computeGraceRemaining ─────────────────────────────────────────

describe("computeGraceRemaining", () => {
  // New signature: computeGraceRemaining(elapsedSecs, timeoutSecs, receivedAtMs, nowMs)
  // remaining = timeoutSecs - effectiveElapsed(elapsedSecs, receivedAt, now)

  it("returns full grace period when no local time has passed", () => {
    const now = 1000000;
    // Grace=10, stop steps took 5s → elapsed=5, timeout=15
    expect(computeGraceRemaining(5, 15, now, now)).toBe(10);
  });

  it("counts down as local time passes", () => {
    const received = 1000000;
    // Grace=10, elapsed=0 at start of grace, timeout=10
    expect(computeGraceRemaining(0, 10, received, received + 1000)).toBe(9);
    expect(computeGraceRemaining(0, 10, received, received + 5000)).toBe(5);
    expect(computeGraceRemaining(0, 10, received, received + 9000)).toBe(1);
  });

  it("reaches zero exactly when grace period elapses", () => {
    const received = 1000000;
    expect(computeGraceRemaining(0, 10, received, received + 10000)).toBe(0);
  });

  it("never goes below zero", () => {
    const received = 1000000;
    expect(computeGraceRemaining(0, 10, received, received + 20000)).toBe(0);
  });

  it("handles fractional local deltas", () => {
    const received = 1000000;
    // 1.5s passed → 8.5s remaining
    expect(computeGraceRemaining(0, 10, received, received + 1500)).toBe(8.5);
  });

  it("clamps negative local delta to zero (clock skew)", () => {
    const received = 1000000;
    // Clock went backwards — should still return full grace period
    expect(computeGraceRemaining(0, 10, received, received - 500)).toBe(10);
  });

  it("handles a 30s grace period (realistic Minecraft stop)", () => {
    const received = 2000000;
    // Stop steps took 15s → elapsed=15, timeout=45 (15+30 grace)
    expect(computeGraceRemaining(15, 45, received, received)).toBe(30);
    expect(computeGraceRemaining(15, 45, received, received + 10000)).toBe(20);
    expect(computeGraceRemaining(15, 45, received, received + 29000)).toBe(1);
    expect(computeGraceRemaining(15, 45, received, received + 30000)).toBe(0);
  });

  it("returns zero immediately when timeout equals elapsed", () => {
    const now = 1000000;
    expect(computeGraceRemaining(10, 10, now, now)).toBe(0);
  });

  it("stays stable across rapid WS updates (no reset)", () => {
    // Simulate two WS messages 1s apart, both during the same grace period.
    // Grace=30, stop steps took 10s → timeout=40.
    // First message: elapsed=20 (10s into grace), received at t=0
    expect(computeGraceRemaining(20, 40, 1000000, 1000000)).toBe(20);
    // 0.5s of local time passes
    expect(computeGraceRemaining(20, 40, 1000000, 1000500)).toBe(19.5);
    // New message arrives 1s later: elapsed=21, received resets
    expect(computeGraceRemaining(21, 40, 1001000, 1001000)).toBe(19);
    // 0.5s of local time after second message
    expect(computeGraceRemaining(21, 40, 1001000, 1001500)).toBe(18.5);
  });
});

// ─── computeGracePercent ────────────────────────────────────────────

describe("computeGracePercent", () => {
  // New signature: computeGracePercent(elapsedSecs, timeoutSecs, graceSecs, receivedAtMs, nowMs)
  // graceStart = timeoutSecs - graceSecs
  // graceElapsed = max(0, effectiveElapsed - graceStart)
  // percent = graceElapsed / graceSecs * 100

  it("returns 0 when no local time has passed and grace just started", () => {
    const now = 1000000;
    // Grace=10, stop steps took 5s → elapsed=5, timeout=15
    expect(computeGracePercent(5, 15, 10, now, now)).toBe(0);
  });

  it("returns 50 at the halfway point", () => {
    const received = 1000000;
    // Grace=10, elapsed=0 at grace start, timeout=10, 5s local → 50%
    expect(computeGracePercent(0, 10, 10, received, received + 5000)).toBe(50);
  });

  it("returns 100 when the grace period has fully elapsed", () => {
    const received = 1000000;
    expect(computeGracePercent(0, 10, 10, received, received + 10000)).toBe(
      100,
    );
  });

  it("clamps to 100 when past the grace period", () => {
    const received = 1000000;
    expect(computeGracePercent(0, 10, 10, received, received + 20000)).toBe(
      100,
    );
  });

  it("returns 100 when grace is zero", () => {
    const now = 1000000;
    expect(computeGracePercent(10, 10, 0, now, now)).toBe(100);
  });

  it("returns 100 when grace is negative", () => {
    const now = 1000000;
    expect(computeGracePercent(10, 10, -1, now, now)).toBe(100);
  });

  it("progresses second by second", () => {
    const received = 1000000;
    // Grace=20, elapsed=0 at grace start, timeout=20
    expect(computeGracePercent(0, 20, 20, received, received + 2000)).toBe(10);
    expect(computeGracePercent(0, 20, 20, received, received + 10000)).toBe(50);
    expect(computeGracePercent(0, 20, 20, received, received + 20000)).toBe(
      100,
    );
  });

  it("handles fractional results", () => {
    const received = 1000000;
    // Grace=3, elapsed=0, timeout=3 → 1s of 3s grace = 33.33...%
    const pct = computeGracePercent(0, 3, 3, received, received + 1000);
    expect(pct).toBeCloseTo(33.333, 2);
  });

  it("accounts for stop-step elapsed time in the overall timeline", () => {
    const received = 1000000;
    // Stop steps took 20s, grace=30s → timeout=50
    // At received: elapsed=20 → grace just started → 0%
    expect(computeGracePercent(20, 50, 30, received, received)).toBe(0);
    // 15s local → elapsed effectively 35 → 15s into 30s grace → 50%
    expect(computeGracePercent(20, 50, 30, received, received + 15000)).toBe(
      50,
    );
    // 30s local → elapsed effectively 50 → 30s into 30s grace → 100%
    expect(computeGracePercent(20, 50, 30, received, received + 30000)).toBe(
      100,
    );
  });

  it("stays stable across rapid WS updates (no reset)", () => {
    // Grace=30, steps took 10s → timeout=40.
    // First message: elapsed=20 (10s into grace)
    expect(computeGracePercent(20, 40, 30, 1000000, 1000000)).toBeCloseTo(
      33.333,
      2,
    );
    // New message 1s later: elapsed=21, receivedAt resets
    expect(computeGracePercent(21, 40, 30, 1001000, 1001000)).toBeCloseTo(
      36.667,
      2,
    );
    // 0.5s after second message
    expect(computeGracePercent(21, 40, 30, 1001000, 1001500)).toBeCloseTo(
      38.333,
      2,
    );
  });
});
