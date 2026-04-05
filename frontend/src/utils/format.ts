export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

export function formatUptime(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  const parts: string[] = [];
  if (d > 0) parts.push(`${d}d`);
  if (h > 0) parts.push(`${h}h`);
  if (m > 0) parts.push(`${m}m`);
  parts.push(`${s}s`);
  return parts.join(" ");
}

/** Interpolates elapsed seconds between WebSocket updates using the local clock. */
export function effectiveElapsed(
  elapsedSecs: number,
  receivedAtMs: number,
  nowMs: number,
): number {
  const localDelta = Math.max(0, (nowMs - receivedAtMs) / 1000);
  return elapsedSecs + localDelta;
}

export function computeShutdownRemaining(
  elapsedSecs: number,
  timeoutSecs: number,
  receivedAtMs: number,
  nowMs: number,
): number {
  return Math.max(
    0,
    timeoutSecs - effectiveElapsed(elapsedSecs, receivedAtMs, nowMs),
  );
}

export function computeShutdownPercent(
  elapsedSecs: number,
  timeoutSecs: number,
  receivedAtMs: number,
  nowMs: number,
): number {
  const effective = effectiveElapsed(elapsedSecs, receivedAtMs, nowMs);
  if (timeoutSecs <= 0) return 100;
  return Math.min(100, (effective / timeoutSecs) * 100);
}

/**
 * Compute how many seconds remain in the grace period.
 *
 * Uses the server-reported `elapsedSecs` (anchored to stop-start) and
 * `timeoutSecs` (total estimated time until SIGKILL) so the countdown
 * is stable across WebSocket updates.  A local-clock delta from
 * `receivedAtMs` interpolates smoothly between server messages.
 */
export function computeGraceRemaining(
  elapsedSecs: number,
  timeoutSecs: number,
  receivedAtMs: number,
  nowMs: number,
): number {
  return Math.max(
    0,
    timeoutSecs - effectiveElapsed(elapsedSecs, receivedAtMs, nowMs),
  );
}

/**
 * Compute the grace-period progress as a percentage (0–100).
 *
 * `graceStart` is derived as `timeoutSecs - graceSecs` (the point in the
 * overall timeline where the grace period began).  Progress is measured
 * from that anchor so it is immune to `receivedAt` resets.
 */
export function computeGracePercent(
  elapsedSecs: number,
  timeoutSecs: number,
  graceSecs: number,
  receivedAtMs: number,
  nowMs: number,
): number {
  if (graceSecs <= 0) return 100;
  const effective = effectiveElapsed(elapsedSecs, receivedAtMs, nowMs);
  const graceStart = timeoutSecs - graceSecs;
  const graceElapsed = Math.max(0, effective - graceStart);
  return Math.min(100, (graceElapsed / graceSecs) * 100);
}

/** Format a date string as "Jan 1, 2024" (date only). */
export function formatDate(dateStr: string): string {
  try {
    return new Date(dateStr).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  } catch {
    return dateStr;
  }
}

/** Format a date string as "Jan 1, 2:30 PM" (date + time, no year). */
export function formatDateTime(dateStr: string | null): string {
  if (!dateStr) return "";
  try {
    return new Date(dateStr).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return dateStr ?? "";
  }
}

/** Format a date string as "YYYY-MM-DD HH:MM". */
export function formatDateTimePadded(dateStr: string): string {
  try {
    const date = new Date(dateStr);
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    const hours = String(date.getHours()).padStart(2, "0");
    const minutes = String(date.getMinutes()).padStart(2, "0");
    return `${year}-${month}-${day} ${hours}:${minutes}`;
  } catch {
    return dateStr;
  }
}

/** Map a percentage to a threshold severity class name ("ok", "warning", or "critical"). */
export function thresholdClass(pct: number): string {
  if (pct >= 90) return "critical";
  if (pct >= 70) return "warning";
  return "ok";
}

export function formatShutdownCountdown(remainingSecs: number): string {
  const secs = Math.ceil(Math.max(0, remainingSecs));
  if (secs >= 60) {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s}s`;
  }
  return `${secs}s`;
}
