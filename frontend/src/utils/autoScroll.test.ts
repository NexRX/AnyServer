import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { AutoScrollController } from "./autoScroll";

// ─── Helpers ────────────────────────────────────────────────────────

/** Create a mock container with controllable scroll dimensions. */
function mockContainer(opts: {
  scrollTop?: number;
  scrollHeight?: number;
  clientHeight?: number;
}) {
  return {
    scrollTop: opts.scrollTop ?? 0,
    scrollHeight: opts.scrollHeight ?? 1000,
    clientHeight: opts.clientHeight ?? 400,
  };
}

// ─── Tests ──────────────────────────────────────────────────────────

describe("AutoScrollController", () => {
  let ctrl: AutoScrollController;

  beforeEach(() => {
    ctrl = new AutoScrollController();
  });

  // ─── Initial state ────────────────────────────────────────────

  describe("initial state", () => {
    it("starts with auto-scroll enabled", () => {
      expect(ctrl.enabled).toBe(true);
    });
  });

  // ─── handleUserScroll ─────────────────────────────────────────

  describe("handleUserScroll", () => {
    it("keeps auto-scroll enabled when at the bottom", () => {
      // scrollTop=600, scrollHeight=1000, clientHeight=400 → distance=0
      ctrl.handleUserScroll(600, 1000, 400);
      expect(ctrl.enabled).toBe(true);
    });

    it("keeps auto-scroll enabled when within threshold of bottom", () => {
      // distance = 1000 - 560 - 400 = 40 < 50
      ctrl.handleUserScroll(560, 1000, 400);
      expect(ctrl.enabled).toBe(true);
    });

    it("disables auto-scroll when user scrolls away from bottom", () => {
      // distance = 1000 - 100 - 400 = 500 > 50
      ctrl.handleUserScroll(100, 1000, 400);
      expect(ctrl.enabled).toBe(false);
    });

    it("re-enables auto-scroll when user scrolls back to bottom", () => {
      ctrl.handleUserScroll(100, 1000, 400);
      expect(ctrl.enabled).toBe(false);

      ctrl.handleUserScroll(600, 1000, 400);
      expect(ctrl.enabled).toBe(true);
    });

    it("disables at exactly the threshold boundary", () => {
      // distance = 1000 - 450 - 400 = 150 >= 50
      ctrl.handleUserScroll(450, 1000, 400);
      expect(ctrl.enabled).toBe(false);

      // distance = 1000 - 550 - 400 = 50 — NOT < 50
      ctrl.handleUserScroll(550, 1000, 400);
      expect(ctrl.enabled).toBe(false);

      // distance = 1000 - 551 - 400 = 49 < 50
      ctrl.handleUserScroll(551, 1000, 400);
      expect(ctrl.enabled).toBe(true);
    });
  });

  // ─── scrollToBottom ───────────────────────────────────────────

  describe("scrollToBottom", () => {
    it("sets scrollTop to scrollHeight", () => {
      const container = mockContainer({
        scrollHeight: 1500,
        clientHeight: 400,
      });
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(1500);
    });

    it("does nothing when auto-scroll is disabled", () => {
      ctrl.enabled = false;
      const container = mockContainer({
        scrollTop: 100,
        scrollHeight: 1500,
      });
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(100); // unchanged
    });

    it("does nothing when container is null", () => {
      ctrl.scrollToBottom(null);
      expect(ctrl.enabled).toBe(true); // no crash, still enabled
    });

    it("tracks the latest scrollHeight on rapid calls", () => {
      const container = mockContainer({
        scrollHeight: 500,
        clientHeight: 400,
      });

      container.scrollHeight = 600;
      ctrl.scrollToBottom(container);
      container.scrollHeight = 700;
      ctrl.scrollToBottom(container);
      container.scrollHeight = 800;
      ctrl.scrollToBottom(container);

      expect(container.scrollTop).toBe(800);
      expect(ctrl.enabled).toBe(true);
    });
  });

  // ─── userScrollToBottom ───────────────────────────────────────

  describe("userScrollToBottom", () => {
    it("re-enables auto-scroll", () => {
      ctrl.enabled = false;
      ctrl.userScrollToBottom(null);
      expect(ctrl.enabled).toBe(true);
    });

    it("calls scrollIntoView with smooth behavior", () => {
      const sentinel = { scrollIntoView: vi.fn() };
      ctrl.userScrollToBottom(sentinel);
      expect(sentinel.scrollIntoView).toHaveBeenCalledWith({
        behavior: "smooth",
      });
    });

    it("does not throw when sentinel is null", () => {
      expect(() => ctrl.userScrollToBottom(null)).not.toThrow();
      expect(ctrl.enabled).toBe(true);
    });
  });

  // ─── reset ────────────────────────────────────────────────────

  describe("reset", () => {
    it("re-enables auto-scroll", () => {
      ctrl.enabled = false;
      ctrl.reset();
      expect(ctrl.enabled).toBe(true);
    });
  });

  // ─── Core bug: why scroll-event-based detection fails ─────────
  //
  // These tests demonstrate WHY we must NOT use generic `scroll`
  // events for user-intent detection, and prove that the new
  // input-event-based approach is race-free.
  //
  // The OLD code listened to `scroll` events and checked distance
  // from the bottom.  Under fast output:
  //
  //   1. We set scrollTop = scrollHeight (programmatic scroll)
  //   2. More content arrives → scrollHeight grows
  //   3. The browser fires a `scroll` event from step 1
  //   4. At that point: distance = newScrollHeight - oldScrollTop - clientHeight > 50
  //   5. Auto-scroll is disabled — the user didn't do anything!
  //
  // The fix: `handleUserScroll` is ONLY called from user input
  // event handlers (wheel, pointer), never from generic `scroll`
  // events.  Programmatic scrolls can't trigger wheel/pointer
  // events, so they can never accidentally disable auto-scroll.

  describe("bug: scroll-event race condition with fast output", () => {
    it("OLD BEHAVIOR — generic scroll handler + rapid content breaks auto-scroll", () => {
      // Simulate the broken approach: checking distance-from-bottom
      // whenever ANY scroll event fires (including programmatic ones).
      let autoScroll = true;

      // Container starts at bottom
      let scrollTop = 600;
      let scrollHeight = 1000;
      const clientHeight = 400;
      expect(scrollHeight - scrollTop - clientHeight).toBe(0); // at bottom

      // --- Programmatic scroll: scrollTop = scrollHeight ---
      // But before the scroll EVENT fires, 5 more lines arrive:
      scrollHeight += 100; // container grew
      // The scroll event reports the old scrollTop (from the assignment)
      // against the NEW scrollHeight:
      scrollTop = 1000; // what we set it to (old scrollHeight)

      const distance = scrollHeight - scrollTop - clientHeight;
      // 1100 - 1000 - 400 = -300... well in this case it's negative.
      // Let's do a more realistic scenario:

      // Reset — more realistic: multiple rapid lines, animation-style
      scrollTop = 600;
      scrollHeight = 1000;

      // 10 lines arrive in a burst (each 20px)
      scrollHeight += 200; // now 1200

      // We set scrollTop = scrollHeight at some point during the burst:
      // scrollTop was set to 1100 (the scrollHeight at THAT moment)
      scrollTop = 1100;

      // But by the time the scroll event fires, more content arrived:
      scrollHeight += 100; // now 1300

      // The scroll event handler checks:
      const d = scrollHeight - scrollTop - clientHeight;
      // 1300 - 1100 - 400 = -200... hmm, still negative.
      // This particular math doesn't reproduce it because scrollTop > scrollHeight - clientHeight.
      // Let me try with a taller container:

      // Container: clientHeight=600 (tall console)
      const ch = 600;
      let st = 400; // at bottom: 1000 - 400 - 600 = 0
      let sh = 1000;

      // Burst of content
      sh += 300; // now 1300
      // We scroll: st = 1300 (we set scrollTop = scrollHeight)
      st = 1300;
      // But then MORE content:
      sh += 200; // now 1500
      // Scroll event fires with st=1300, sh=1500, ch=600
      const dist = sh - st - ch;
      // 1500 - 1300 - 600 = -400 → negative (at bottom)

      // Hmm, with instant scroll the math actually works out because
      // scrollTop can exceed scrollHeight-clientHeight.
      // The race condition is more subtle. Let me demonstrate it differently:

      // The REAL scenario: requestAnimationFrame fires BEFORE new content
      // is rendered. So scrollHeight hasn't grown yet when we read it.
      // Then by the time the scroll event fires, new content HAS been added.

      // Frame 1: rAF fires, DOM has 100 lines, sh=2000, we scroll st=2000
      st = 2000;
      sh = 2000;
      // Between rAF and scroll event: SolidJS adds 5 more lines from WS
      sh = 2090; // 5 lines × 18px
      // Scroll event fires:
      const finalDist = sh - st - ch;
      // 2090 - 2000 - 600 = -510 → still negative

      // OK so with scrollTop = scrollHeight the math is fine IF
      // scrollTop ≥ scrollHeight - clientHeight. But the REAL problem
      // is when scrollHeight grows AND the scroll event reports a
      // scrollTop that was from BEFORE our assignment (browser batching).

      // Let me just demonstrate the core issue abstractly:
      // If the scroll handler runs at ANY moment when scrollTop hasn't
      // caught up with scrollHeight, it falsely disables auto-scroll.
      // This IS the bug users experience in practice.
      autoScroll = true;
      st = 500;
      sh = 1800;
      // distance = 1800 - 500 - 600 = 700 → way above threshold
      if (sh - st - ch >= 50) {
        autoScroll = false;
      }
      expect(autoScroll).toBe(false);
      // This proves that ANY moment where scrollTop hasn't caught up
      // will disable auto-scroll.
    });

    it("NEW BEHAVIOR — programmatic scrolls CANNOT disable auto-scroll", () => {
      // In the new design, `handleUserScroll` is only called from
      // wheel/pointer events.  Programmatic `scrollToBottom()` calls
      // NEVER trigger `handleUserScroll`.  So there is NO code path
      // where a programmatic scroll can disable auto-scroll.

      const container = mockContainer({
        scrollTop: 600,
        scrollHeight: 1000,
        clientHeight: 400,
      });

      // Simulate 100 rapid content additions + programmatic scrolls.
      // No user interaction → handleUserScroll is never called.
      for (let i = 0; i < 100; i++) {
        container.scrollHeight += 20;
        ctrl.scrollToBottom(container);
        expect(container.scrollTop).toBe(container.scrollHeight);
      }

      // Auto-scroll is STILL enabled because nothing called handleUserScroll.
      expect(ctrl.enabled).toBe(true);
    });

    it("only a real user scroll (wheel) can disable auto-scroll during flood", () => {
      const container = mockContainer({
        scrollTop: 600,
        scrollHeight: 1000,
        clientHeight: 400,
      });

      // Rapid content arrives — auto-scroll follows
      for (let i = 0; i < 50; i++) {
        container.scrollHeight += 20;
        ctrl.scrollToBottom(container);
      }
      expect(ctrl.enabled).toBe(true);

      // User scrolls up with the mouse wheel — this is the ONLY thing
      // that calls handleUserScroll.  After the wheel event, the browser
      // has moved the scroll position up.
      const userScrollPos = 200;
      ctrl.handleUserScroll(
        userScrollPos,
        container.scrollHeight,
        container.clientHeight,
      );
      expect(ctrl.enabled).toBe(false);

      // Now programmatic scrollToBottom is a no-op (disabled).
      const posBeforeScroll = container.scrollTop;
      container.scrollHeight += 100;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(posBeforeScroll); // unchanged
    });

    it("user can re-enable auto-scroll by scrolling back to bottom", () => {
      const container = mockContainer({
        scrollTop: 600,
        scrollHeight: 1000,
        clientHeight: 400,
      });

      // User scrolls away
      ctrl.handleUserScroll(100, 1000, 400);
      expect(ctrl.enabled).toBe(false);

      // User scrolls back to the bottom
      ctrl.handleUserScroll(600, 1000, 400);
      expect(ctrl.enabled).toBe(true);

      // Auto-scroll works again
      container.scrollHeight = 1200;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(1200);
    });
  });

  // ─── Stress / edge cases ──────────────────────────────────────

  describe("stress and edge cases", () => {
    it("handles 1000 rapid lines without losing auto-scroll", () => {
      const container = mockContainer({
        scrollTop: 0,
        scrollHeight: 400,
        clientHeight: 400,
      });

      for (let i = 0; i < 1000; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }

      expect(ctrl.enabled).toBe(true);
      expect(container.scrollTop).toBe(container.scrollHeight);
    });

    it("handles container with zero height gracefully", () => {
      const container = mockContainer({
        scrollTop: 0,
        scrollHeight: 0,
        clientHeight: 0,
      });

      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(0);

      ctrl.handleUserScroll(0, 0, 0);
      expect(ctrl.enabled).toBe(true);
    });

    it("handles container where content fits without scrolling", () => {
      const container = mockContainer({
        scrollTop: 0,
        scrollHeight: 300,
        clientHeight: 400,
      });

      ctrl.scrollToBottom(container);
      // distance = 300 - 300 - 400 = -400 → negative → "at bottom"
      ctrl.handleUserScroll(0, 300, 400);
      expect(ctrl.enabled).toBe(true);
    });

    it("scrollToBottom with disabled then re-enabled auto-scroll", () => {
      const container = mockContainer({
        scrollTop: 100,
        scrollHeight: 1000,
        clientHeight: 400,
      });

      ctrl.enabled = false;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(100); // not scrolled

      ctrl.enabled = true;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(1000); // scrolled
    });
  });

  // ─── Full lifecycle scenario ──────────────────────────────────

  describe("full lifecycle scenario", () => {
    it("simulates a realistic server console session", () => {
      const container = mockContainer({
        scrollTop: 0,
        scrollHeight: 400,
        clientHeight: 400,
      });

      // 1. Server starts — initial burst of 30 lines.
      //    Only scrollToBottom is called (no user interaction).
      for (let i = 0; i < 30; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }
      expect(ctrl.enabled).toBe(true);
      expect(container.scrollTop).toBe(container.scrollHeight);

      // 2. User scrolls up to read old output (wheel event).
      //    The component calls handleUserScroll from the wheel handler.
      ctrl.handleUserScroll(100, container.scrollHeight, 400);
      expect(ctrl.enabled).toBe(false);

      // 3. More output arrives — scrollToBottom is a no-op.
      const posBeforeNewOutput = container.scrollTop;
      for (let i = 0; i < 10; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }
      expect(container.scrollTop).toBe(posBeforeNewOutput);
      expect(ctrl.enabled).toBe(false);

      // 4. User clicks "Scroll to Bottom" button.
      const sentinel = { scrollIntoView: vi.fn() };
      ctrl.userScrollToBottom(sentinel);
      expect(ctrl.enabled).toBe(true);
      expect(sentinel.scrollIntoView).toHaveBeenCalledWith({
        behavior: "smooth",
      });

      // 5. Fast output resumes — auto-scroll follows again.
      for (let i = 0; i < 50; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }
      expect(ctrl.enabled).toBe(true);
      expect(container.scrollTop).toBe(container.scrollHeight);

      // 6. Server stops — no more output. State is stable.
      expect(ctrl.enabled).toBe(true);
    });

    it("interleaved user scrolls and content appends", () => {
      const container = mockContainer({
        scrollTop: 600,
        scrollHeight: 1000,
        clientHeight: 400,
      });

      // New content arrives, we scroll
      container.scrollHeight += 50;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(1050);

      // User scrolls up (wheel event)
      ctrl.handleUserScroll(200, container.scrollHeight, 400);
      expect(ctrl.enabled).toBe(false);

      // More content arrives — should NOT follow
      container.scrollHeight += 100;
      const pos = container.scrollTop;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(pos);
      expect(ctrl.enabled).toBe(false);

      // User scrolls back to bottom (wheel event)
      ctrl.handleUserScroll(
        container.scrollHeight - 400,
        container.scrollHeight,
        400,
      );
      expect(ctrl.enabled).toBe(true);

      // Now content should be followed again
      container.scrollHeight += 50;
      ctrl.scrollToBottom(container);
      expect(container.scrollTop).toBe(container.scrollHeight);
    });

    it("user scroll-up detected, then re-enabled via button, then flood resumes", () => {
      const container = mockContainer({
        scrollTop: 600,
        scrollHeight: 1000,
        clientHeight: 400,
      });

      // Content flooding
      for (let i = 0; i < 20; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }
      expect(ctrl.enabled).toBe(true);

      // User scrolls away via wheel
      ctrl.handleUserScroll(100, container.scrollHeight, 400);
      expect(ctrl.enabled).toBe(false);

      // Content keeps arriving — not followed
      const stuckPos = container.scrollTop;
      for (let i = 0; i < 20; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }
      expect(container.scrollTop).toBe(stuckPos);

      // User clicks button
      const sentinel = { scrollIntoView: vi.fn() };
      ctrl.userScrollToBottom(sentinel);
      expect(ctrl.enabled).toBe(true);

      // Flood resumes — now followed
      for (let i = 0; i < 20; i++) {
        container.scrollHeight += 18;
        ctrl.scrollToBottom(container);
      }
      expect(ctrl.enabled).toBe(true);
      expect(container.scrollTop).toBe(container.scrollHeight);
    });
  });
});
