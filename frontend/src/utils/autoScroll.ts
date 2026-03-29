// Detects user scroll intent via input events (wheel, pointer, keyboard)
// rather than `scroll` events, which also fire for programmatic scrolls
// and cause false negatives under fast output.

const AT_BOTTOM_THRESHOLD = 50;

export class AutoScrollController {
  private _enabled = true;

  get enabled(): boolean {
    return this._enabled;
  }

  set enabled(value: boolean) {
    this._enabled = value;
  }

  handleUserScroll(
    scrollTop: number,
    scrollHeight: number,
    clientHeight: number,
  ): void {
    const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
    this._enabled = distanceFromBottom < AT_BOTTOM_THRESHOLD;
  }

  scrollToBottom(
    container: { scrollTop: number; scrollHeight: number } | null,
  ): void {
    if (!this._enabled || !container) return;
    container.scrollTop = container.scrollHeight;
  }

  userScrollToBottom(
    sentinel: { scrollIntoView: (opts: ScrollIntoViewOptions) => void } | null,
  ): void {
    this._enabled = true;
    if (!sentinel) return;
    sentinel.scrollIntoView({ behavior: "smooth" });
  }

  reset(): void {
    this._enabled = true;
  }
}
