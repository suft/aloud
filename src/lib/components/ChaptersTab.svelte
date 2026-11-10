<script lang="ts">
  import Play from "@lucide/svelte/icons/play";
  import { api, type ChapterDetail } from "$lib/api";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Checkbox } from "$lib/components/ui/checkbox";
  import { ScrollArea } from "$lib/components/ui/scroll-area";
  import { formatDuration } from "$lib/format";
  import type { BookView } from "$lib/stores/book.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { attempt } from "$lib/toast";
  import EditableText from "./EditableText.svelte";

  let { book }: { book: BookView } = $props();

  const detail = $derived(book.detail!);
  const live = $derived(render.progress[book.id]?.chapters ?? []);

  function select(include: (chapter: ChapterDetail) => boolean) {
    const changes = detail.chapters
      .filter((c) => c.included !== include(c))
      .map((c) => ({ id: c.id, included: include(c) }));
    if (changes.length > 0) void book.patch({ chapters: changes });
  }

  function liveFor(chapter: ChapterDetail) {
    const progress = live.find((c) => c.id === chapter.id);
    return progress && progress.totalUnits > 0 ? progress : null;
  }

  function renderedFraction(chapter: ChapterDetail): number {
    const progress = liveFor(chapter);
    const liveFraction = progress ? progress.doneUnits / progress.totalUnits : 0;
    return Math.max(chapter.renderedFraction, liveFraction);
  }

  /** Playable once the saved state says so, or as soon as live progress shows it complete. */
  function isPlayable(chapter: ChapterDetail): boolean {
    const progress = liveFor(chapter);
    return chapter.playable || (progress !== null && progress.doneUnits >= progress.totalUnits);
  }
</script>

<div class="space-y-3">
  <div class="flex flex-wrap items-center gap-2">
    <Button variant="outline" size="sm" disabled={book.locked} onclick={() => select(() => true)}>
      All
    </Button>
    <Button variant="outline" size="sm" disabled={book.locked} onclick={() => select(() => false)}>
      None
    </Button>
    <Button
      variant="outline"
      size="sm"
      disabled={book.locked}
      onclick={() => select((c) => c.kind === "body")}
    >
      Main chapters only
    </Button>
  </div>

  <ScrollArea class="h-[460px] rounded-2xl border border-border/60 bg-card/80 shadow-sm">
    <ul class="divide-y divide-border/60">
      {#each detail.chapters as chapter, i (chapter.id)}
        {@const fraction = renderedFraction(chapter)}
        <li class="flex items-center gap-3 px-4 py-2.5">
          <Checkbox
            aria-label={`Include ${chapter.title}`}
            disabled={book.locked}
            bind:checked={
              () => chapter.included,
              (included) => book.patch({ chapters: [{ id: chapter.id, included }] })
            }
          />
          <span class="w-6 text-right text-xs text-muted-foreground">{i + 1}</span>
          <div class="min-w-0 flex-1 truncate text-sm">
            <EditableText
              value={chapter.title}
              disabled={book.locked}
              class={chapter.included ? "" : "text-muted-foreground"}
              onsave={(title) => title && book.patch({ chapters: [{ id: chapter.id, title }] })}
            />
          </div>
          {#if chapter.kind !== "body"}
            <Badge variant="secondary" class="text-muted-foreground">
              {chapter.kind === "frontMatter" ? "Front matter" : "Back matter"}
            </Badge>
          {/if}
          <span class="text-xs whitespace-nowrap text-muted-foreground">
            {chapter.words.toLocaleString()} words · {chapter.estSecs < 60
              ? "< 1 min"
              : `~${formatDuration(chapter.estSecs)}`}
          </span>
          <div class="flex w-14 shrink-0 items-center justify-end">
            {#if isPlayable(chapter)}
              <Button
                variant="ghost"
                size="icon-xs"
                aria-label={`Play ${chapter.title}`}
                onclick={() => attempt(api.playerPlayChapter(book.id, chapter.id, 0))}
              >
                <Play />
              </Button>
            {:else if fraction > 0}
              <div class="h-1 w-12 overflow-hidden rounded-full bg-muted">
                <div
                  class="h-full bg-book transition-[width]"
                  style:width="{fraction * 100}%"
                ></div>
              </div>
            {:else}
              <span class="size-1.5 rounded-full bg-muted-foreground/40"></span>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  </ScrollArea>
</div>
