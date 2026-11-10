<script lang="ts">
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import { fade } from "svelte/transition";
  import { api } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { Progress } from "$lib/components/ui/progress";
  import { formatDuration, percent } from "$lib/format";
  import { dur } from "$lib/motion";
  import type { BookView } from "$lib/stores/book.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { render } from "$lib/stores/render.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let { book }: { book: BookView } = $props();

  let confirmOpen = $state(false);

  const status = $derived(book.detail!.project.status.state);
  const visible = $derived(
    status === "queued" || status === "rendering" || status === "exporting" || status === "paused",
  );
  const progress = $derived(render.progress[book.id]);
  const fraction = $derived(progress?.fraction ?? projects.find(book.id)?.progress ?? 0);
  const position = $derived(render.position(book.id));

  const headline = $derived.by(() => {
    if (status === "paused") return "Stopped — finished parts are saved.";
    if (status === "exporting" || progress?.phase === "exporting") return "Finishing up…";
    if (status === "queued" && position > 0) {
      return `Queued — ${position} book${position === 1 ? "" : "s"} ahead`;
    }
    if (!progress || progress.etaSecs === null) return "Estimating time…";
    return `About ${formatDuration(progress.etaSecs)} left`;
  });

  const details = $derived.by(() => {
    if (status !== "rendering" || progress?.phase !== "rendering") return null;
    const parts: string[] = [];
    const total = progress.chapters.length;
    if (total > 0) {
      const current = progress.chapters.findIndex((c) => c.doneUnits < c.totalUnits);
      parts.push(`Chapter ${current < 0 ? total : current + 1} of ${total}`);
    }
    if (progress.realtimeFactor !== null) {
      parts.push(`${progress.realtimeFactor.toFixed(1)}× faster than real time`);
    }
    return parts.join(" · ");
  });
</script>

{#if visible}
  <div
    class="space-y-4 rounded-2xl border border-border/60 bg-card/80 p-6 shadow-sm backdrop-blur"
    transition:fade={{ duration: dur(150) }}
  >
    <div class="flex items-end justify-between gap-4">
      <div class="text-5xl font-semibold tracking-tight">{percent(fraction)}%</div>
      <div class="pb-1 text-right text-sm text-muted-foreground">
        <div class="text-base text-foreground">{headline}</div>
        {#if details}<div>{details}</div>{/if}
      </div>
    </div>
    <Progress value={fraction} max={1} class="h-2 [&>div]:bg-book" />
    <div class="flex gap-2">
      {#if status !== "paused"}
        {#if render.queue.paused}
          <Button variant="outline" size="sm" onclick={() => render.resume()}>
            <Play /> Resume
          </Button>
        {:else}
          <Button variant="outline" size="sm" onclick={() => render.apply(api.pauseQueue())}>
            <Pause /> Pause
          </Button>
        {/if}
      {/if}
      {#if position >= 0}
        <Button variant="ghost" size="sm" onclick={() => (confirmOpen = true)}>Cancel</Button>
      {/if}
    </div>
  </div>
{/if}

<ConfirmDialog
  bind:open={confirmOpen}
  title="Stop making this audiobook?"
  description="Finished parts are saved, so you can continue later."
  confirmLabel="Stop"
  cancelLabel="Keep going"
  destructive
  onconfirm={() => render.apply(api.cancelRender(book.id))}
/>
