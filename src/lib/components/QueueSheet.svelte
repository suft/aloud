<script lang="ts">
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import X from "@lucide/svelte/icons/x";
  import { api } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import * as Sheet from "$lib/components/ui/sheet";
  import { formatTimeLeft, percent } from "$lib/format";
  import { app } from "$lib/stores/app.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { render } from "$lib/stores/render.svelte";
  import Cover from "./Cover.svelte";

  const items = $derived(
    render.queue.items.map((id, index) => ({
      id,
      index,
      book: projects.find(id),
      progress: render.progress[id],
    })),
  );

  function statusText(index: number, progress: (typeof items)[number]["progress"]): string {
    if (index > 0) return `Waiting · #${index + 1}`;
    if (render.queue.paused) return "Paused";
    if (!progress) return "Starting…";
    if (progress.phase === "exporting") return "Finishing up…";
    const pct = `${percent(progress.fraction)}%`;
    return progress.etaSecs === null ? pct : `${pct} · ${formatTimeLeft(progress.etaSecs)}`;
  }
</script>

<Sheet.Root bind:open={app.queueOpen}>
  <Sheet.Content class="data-[side=right]:sm:max-w-[420px]">
    <Sheet.Header>
      <Sheet.Title>Making audiobooks</Sheet.Title>
      <Sheet.Description>Books are made one at a time, in this order.</Sheet.Description>
      {#if items.length > 0}
        <div class="pt-2">
          {#if render.queue.paused}
            <Button size="sm" onclick={() => render.resume()}><Play /> Resume</Button>
          {:else}
            <Button size="sm" variant="outline" onclick={() => render.apply(api.pauseQueue())}>
              <Pause /> Pause all
            </Button>
          {/if}
        </div>
      {/if}
    </Sheet.Header>

    <div class="flex-1 space-y-2 overflow-y-auto px-4 pb-4">
      {#each items as item (item.id)}
        <div class="flex items-center gap-3 rounded-xl border border-border/60 bg-card/80 p-2.5">
          <div class="h-14 w-10 shrink-0 overflow-hidden rounded-md">
            <Cover
              path={item.book?.coverPath ?? null}
              title={item.book?.title ?? ""}
              accent={item.book?.accent ?? null}
              showTitle={false}
            />
          </div>
          <div class="min-w-0 flex-1">
            <button
              type="button"
              class="block max-w-full truncate text-left text-sm font-medium hover:underline"
              onclick={() => {
                app.queueOpen = false;
                app.go({ name: "book", id: item.id });
              }}
            >
              {item.book?.title ?? "Untitled"}
            </button>
            <div class="text-xs text-muted-foreground">{statusText(item.index, item.progress)}</div>
          </div>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Move up"
            disabled={item.index === 0}
            onclick={() => render.apply(api.moveInQueue(item.id, "up"))}
          >
            <ArrowUp />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Move down"
            disabled={item.index === items.length - 1}
            onclick={() => render.apply(api.moveInQueue(item.id, "down"))}
          >
            <ArrowDown />
          </Button>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label="Remove from queue"
            onclick={() => render.apply(api.cancelRender(item.id))}
          >
            <X />
          </Button>
        </div>
      {:else}
        <p class="py-10 text-center text-sm text-muted-foreground">
          Nothing is being made right now.
        </p>
      {/each}
    </div>
  </Sheet.Content>
</Sheet.Root>
