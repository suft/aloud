<script lang="ts">
  import Play from "@lucide/svelte/icons/play";
  import { api, errorMessage, type TrickyWord } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import type { BookView } from "$lib/stores/book.svelte";
  import { attempt } from "$lib/toast";

  let { book, item }: { book: BookView; item: TrickyWord } = $props();

  let editing = $state(false);
  let soundsLike = $state("");
  let error = $state<string | null>(null);

  function fix() {
    soundsLike = item.word;
    error = null;
    editing = true;
  }

  async function save() {
    try {
      await book.savePronunciation("book", {
        id: crypto.randomUUID(),
        word: item.word,
        sayAs: { kind: "text", value: soundsLike.trim() },
        matchCase: item.word === item.word.toUpperCase(),
      });
      book.dropTricky(item.word);
    } catch (e) {
      error = errorMessage(e);
    }
  }
</script>

<li class="space-y-2 px-4 py-2.5">
  <div class="flex items-center gap-3">
    <span class="font-medium">{item.word}</span>
    <span class="text-xs text-muted-foreground">×{item.count}</span>
    <div class="flex-1"></div>
    <Button
      variant="ghost"
      size="icon-xs"
      aria-label={`Hear “${item.word}” in a sentence`}
      onclick={() => attempt(api.testPronunciation(book.id, item.sample))}
    >
      <Play />
    </Button>
    <Button variant="outline" size="sm" disabled={book.locked} onclick={fix}>Fix</Button>
    <Button
      variant="ghost"
      size="sm"
      disabled={book.locked}
      onclick={() => book.dismissWord(item.word)}
    >
      Looks fine
    </Button>
  </div>
  {#if editing}
    <form
      class="flex items-center gap-2"
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <Input
        bind:value={soundsLike}
        placeholder="Sounds like"
        aria-label="Sounds like"
        class="h-8"
      />
      <Button type="submit" size="sm">Save</Button>
      <Button type="button" variant="ghost" size="sm" onclick={() => (editing = false)}
        >Cancel</Button
      >
    </form>
    {#if error}<p class="text-xs text-destructive">{error}</p>{/if}
  {/if}
</li>
