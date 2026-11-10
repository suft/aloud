<script lang="ts">
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import X from "@lucide/svelte/icons/x";
  import { fly } from "svelte/transition";
  import { api } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { Slider } from "$lib/components/ui/slider";
  import { formatClock } from "$lib/format";
  import { dur } from "$lib/motion";
  import { player } from "$lib/stores/player.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { attempt } from "$lib/toast";
  import Cover from "./Cover.svelte";

  const SKIP_MS = 15_000;

  let scrub = $state<number | null>(null);

  const current = $derived(player.state);
  const book = $derived(projects.find(current?.projectId));

  function skip(deltaMs: number) {
    if (!current) return;
    const target = Math.min(current.durationMs, Math.max(0, current.positionMs + deltaMs));
    void attempt(api.playerSeek(target));
  }

  function commit(ms: number) {
    scrub = null;
    void attempt(api.playerSeek(ms));
  }

  function close() {
    player.state = null;
    void attempt(api.playerStop());
  }
</script>

{#if player.loaded && current}
  <div
    class="flex h-16 shrink-0 items-center gap-4 border-t border-border/60 bg-card/90 px-4 backdrop-blur"
    transition:fly={{ y: 24, duration: dur(180) }}
  >
    <div class="flex w-56 shrink-0 items-center gap-3">
      <div class="h-10 w-7 shrink-0 overflow-hidden rounded-sm">
        <Cover
          path={book?.coverPath ?? null}
          title={book?.title ?? current.title}
          accent={book?.accent ?? null}
          showTitle={false}
        />
      </div>
      <div class="min-w-0">
        <div class="truncate text-sm font-medium">{current.title}</div>
        {#if book}
          <div class="truncate text-xs text-muted-foreground">{book.title}</div>
        {/if}
      </div>
    </div>

    <div class="flex shrink-0 items-center gap-1">
      <Button
        variant="ghost"
        size="icon"
        aria-label="Back 15 seconds"
        onclick={() => skip(-SKIP_MS)}
      >
        <span class="relative grid place-items-center">
          <RotateCcw class="size-5" />
          <span class="absolute text-[8px] font-semibold">15</span>
        </span>
      </Button>
      <Button
        size="icon"
        class="rounded-full"
        aria-label={current.playing ? "Pause" : "Play"}
        onclick={() => attempt(api.playerToggle())}
      >
        {#if current.playing}<Pause />{:else}<Play />{/if}
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label="Forward 15 seconds"
        onclick={() => skip(SKIP_MS)}
      >
        <span class="relative grid place-items-center">
          <RotateCw class="size-5" />
          <span class="absolute text-[8px] font-semibold">15</span>
        </span>
      </Button>
    </div>

    <span class="w-14 shrink-0 text-right text-xs text-muted-foreground">
      {formatClock(scrub ?? current.positionMs)}
    </span>
    <Slider
      type="single"
      value={scrub ?? current.positionMs}
      min={0}
      max={Math.max(1, current.durationMs)}
      step={1000}
      onValueChange={(v) => (scrub = v)}
      onValueCommit={commit}
      aria-label="Position"
    />
    <span class="w-14 shrink-0 text-xs text-muted-foreground"
      >{formatClock(current.durationMs)}</span
    >

    <Button variant="ghost" size="icon-sm" aria-label="Close player" onclick={close}>
      <X />
    </Button>
  </div>
{/if}
