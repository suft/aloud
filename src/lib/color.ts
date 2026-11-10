const HEX = /^#[0-9a-f]{6}$/i;

function channel(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG 2 relative luminance of a `#rrggbb` colour. */
function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => channel(parseInt(hex.slice(i, i + 2), 16) / 255));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * Black or white, whichever reads better on `hex` (at least 4.5:1 for any colour). Anything that
 * isn't `#rrggbb` gets the theme's own foreground.
 */
export function textOn(hex: string): string {
  if (!HEX.test(hex)) return "var(--primary-foreground)";
  const l = luminance(hex);
  const white = 1.05 / (l + 0.05);
  const black = (l + 0.05) / 0.05;
  return black >= white ? "#000000" : "#ffffff";
}
