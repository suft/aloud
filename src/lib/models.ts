import type { ModelVariant } from "./api";

export const MODEL_INFO: Record<
  ModelVariant,
  { name: string; blurb: string; size: string; recommended: boolean }
> = {
  standard: {
    name: "Natural",
    blurb: "Best-sounding narration",
    size: "354 MB",
    recommended: true,
  },
  fast: {
    name: "Fast",
    blurb: "Smaller download",
    size: "121 MB",
    recommended: false,
  },
};

export const MODEL_VARIANTS: ModelVariant[] = ["standard", "fast"];
