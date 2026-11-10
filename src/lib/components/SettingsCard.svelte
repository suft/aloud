<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, children }: { title: string; children: Snippet } = $props();
</script>

<section
  class="space-y-4 rounded-2xl border border-border/60 bg-card/80 p-6 shadow-sm backdrop-blur"
>
  <h2 class="text-base font-semibold">{title}</h2>
  {@render children()}
</section>
