<script lang="ts">
  import Plus from "@lucide/svelte/icons/plus";
  import { errorMessage, type PronScope } from "$lib/api";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import { app } from "$lib/stores/app.svelte";
  import type { BookView } from "$lib/stores/book.svelte";
  import PronRow from "./PronRow.svelte";
  import TrickyRow from "./TrickyRow.svelte";

  let { book }: { book: BookView } = $props();

  let adding = $state(false);
  let newWord = $state("");
  let newSoundsLike = $state("");
  let addError = $state<string | null>(null);

  const fixes = $derived([
    ...book.detail!.project.pronunciations.map((entry) => ({ scope: "book" as PronScope, entry })),
    ...app.settings.pronunciations.map((entry) => ({ scope: "global" as PronScope, entry })),
  ]);

  $effect(() => {
    void book.loadTricky();
  });

  async function add() {
    addError = null;
    try {
      await book.savePronunciation("book", {
        id: crypto.randomUUID(),
        word: newWord.trim(),
        sayAs: { kind: "text", value: newSoundsLike.trim() },
        matchCase: false,
      });
      adding = false;
      newWord = "";
      newSoundsLike = "";
    } catch (e) {
      addError = errorMessage(e);
    }
  }
</script>

<div class="space-y-8">
  {#if book.locked}
    <p class="text-xs text-muted-foreground">Pause first to change this.</p>
  {/if}

  <section class="space-y-3">
    <h3 class="flex items-center gap-2 text-base font-semibold">
      Words to check
      {#if book.tricky}<Badge variant="secondary">{book.tricky.length}</Badge>{/if}
    </h3>
    {#if book.tricky === null}
      <div class="space-y-2 rounded-2xl border border-border/60 bg-card/80 p-4">
        <p class="text-sm text-muted-foreground">Reading the book…</p>
        <Skeleton class="h-8 w-full" />
        <Skeleton class="h-8 w-5/6" />
        <Skeleton class="h-8 w-2/3" />
      </div>
    {:else if book.tricky.length === 0}
      <p class="text-sm text-muted-foreground">No unusual words found.</p>
    {:else}
      <ul
        class="max-h-80 divide-y divide-border/60 overflow-y-auto rounded-2xl border border-border/60 bg-card/80 shadow-sm"
      >
        {#each book.tricky as item (item.word)}
          <TrickyRow {book} {item} />
        {/each}
      </ul>
    {/if}
  </section>

  <section class="space-y-3">
    <div class="flex items-center justify-between">
      <h3 class="text-base font-semibold">Your fixes</h3>
      <Button
        variant="outline"
        size="sm"
        disabled={book.locked || adding}
        onclick={() => (adding = true)}
      >
        <Plus /> Add a word
      </Button>
    </div>

    {#if adding}
      <form
        class="space-y-2 rounded-2xl border border-border/60 bg-card/80 p-4"
        onsubmit={(e) => {
          e.preventDefault();
          void add();
        }}
      >
        <div class="flex flex-wrap items-center gap-2">
          <Input bind:value={newWord} placeholder="Word" aria-label="Word" class="h-8 w-40" />
          <span class="text-muted-foreground">→</span>
          <Input
            bind:value={newSoundsLike}
            placeholder="Sounds like"
            aria-label="Sounds like"
            class="h-8 min-w-40 flex-1"
          />
          <Button type="submit" size="sm">Save</Button>
          <Button type="button" variant="ghost" size="sm" onclick={() => (adding = false)}>
            Cancel
          </Button>
        </div>
        {#if addError}<p class="text-xs text-destructive">{addError}</p>{/if}
      </form>
    {/if}

    {#if fixes.length > 0}
      <ul
        class="divide-y divide-border/60 rounded-2xl border border-border/60 bg-card/80 shadow-sm"
      >
        {#each fixes as { scope, entry } (`${scope}:${entry.id}`)}
          <PronRow {book} {scope} {entry} />
        {/each}
      </ul>
    {/if}

    <p class="text-xs text-muted-foreground">
      Type how it sounds, e.g. Chariandy → Cha-ree-an-dee. Spell letters with spaces: GIS → G I S.
    </p>
  </section>
</div>
