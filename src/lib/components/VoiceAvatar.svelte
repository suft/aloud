<script lang="ts">
  import { cn } from "$lib/utils";

  let { voice, name, class: className }: { voice: string; name: string; class?: string } = $props();

  const hue = $derived([...voice].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7));
</script>

<div
  class={cn(
    "grid size-10 shrink-0 place-items-center rounded-full font-semibold text-white",
    className,
  )}
  style:background="linear-gradient(135deg, oklch(0.72 0.14 {hue}), oklch(0.55 0.19 {hue + 50}))"
>
  {name.slice(0, 1)}
</div>
