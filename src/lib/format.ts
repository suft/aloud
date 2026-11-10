const MB = 1_000_000;

export function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= MB) return `${Math.round(bytes / MB)} MB`;
  if (bytes >= 1000) return `${Math.round(bytes / 1000)} KB`;
  return `${bytes} B`;
}

export function formatSpeed(bytesPerSec: number): string {
  return `${(bytesPerSec / MB).toFixed(1)} MB/s`;
}

/** "3 h 40 min", "18 min", "less than a minute". */
export function formatDuration(secs: number): string {
  const mins = Math.round(secs / 60);
  if (mins < 1) return "less than a minute";
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** "45 s" under a minute, otherwise like `formatDuration`. */
export function formatRemaining(secs: number): string {
  return secs < 60 ? `${Math.max(1, Math.round(secs))} s` : formatDuration(secs);
}

/** "18 min left" / "less than a minute left". */
export function formatTimeLeft(secs: number): string {
  return `${formatDuration(secs)} left`;
}

/** Playback clock: "3:07" or "1:02:09". */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

export function percent(fraction: number): number {
  return Math.min(100, Math.max(0, Math.floor(fraction * 100)));
}

export function parseAuthors(text: string): string[] {
  return text
    .split(",")
    .map((a) => a.trim())
    .filter(Boolean);
}

export const ACCENT_LABEL = { us: "American", uk: "British" } as const;
export const GENDER_LABEL = { female: "Female", male: "Male" } as const;
