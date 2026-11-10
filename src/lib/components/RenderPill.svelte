<script lang="ts">
  import { fly } from "svelte/transition";
  import { formatTimeLeft, percent } from "$lib/format";
  import { dur } from "$lib/motion";
  import { app } from "$lib/stores/app.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { render } from "$lib/stores/render.svelte";

  const progress = $derived(render.headProgress);
  const onItsPage = $derived(app.screen.name === "book" && app.screen.id === render.headId);
  const show = $derived(render.running && progress !== null && !onItsPage);
  const title = $derived(projects.find(render.headId)?.title ?? "your book");
  const detail = $derived.by(() => {
    if (!progress) return "";
    if (progress.phase === "exporting") return "finishing up";
    return progress.etaSecs === null ? "estimating time…" : formatTimeLeft(progress.etaSecs);
  });
</script>

{#if show && progress}
  <div class="flex justify-center px-4 pb-3" transition:fly={{ y: 16, duration: dur(180) }}>
    <button
      type="button"
      class="flex max-w-full items-center gap-2.5 rounded-full border border-border/60 bg-card/90 px-4 py-2 text-sm shadow-lg backdrop-blur hover:bg-card"
      onclick={() => (app.queueOpen = true)}
    >
      <span class="size-2 shrink-0 animate-pulse rounded-full bg-primary"></span>
      <span class="truncate">Making “{title}”</span>
      <span class="shrink-0 text-muted-foreground">
        · {percent(progress.fraction)}% · {detail}
      </span>
    </button>
  </div>
{/if}
