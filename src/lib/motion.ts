const query = window.matchMedia("(prefers-reduced-motion: reduce)");

/** Transition duration in ms, collapsed to zero when the user prefers reduced motion. */
export function dur(ms: number): number {
  return query.matches ? 0 : ms;
}
