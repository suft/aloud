<script lang="ts">
  import Play from "@lucide/svelte/icons/play";
  import Trash from "@lucide/svelte/icons/trash-2";
  import { api, errorMessage, type PronEntry, type PronScope } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Switch } from "$lib/components/ui/switch";
  import * as ToggleGroup from "$lib/components/ui/toggle-group";
  import type { BookView } from "$lib/stores/book.svelte";
  import { attempt } from "$lib/toast";

  let { book, scope, entry }: { book: BookView; scope: PronScope; entry: PronEntry } = $props();

  let value = $derived(entry.sayAs.value);
  let error = $state<string | null>(null);

  /** Runs a save/delete action, keeping the server's message next to the row on failure. */
  async function run(action: () => Promise<void>): Promise<boolean> {
    error = null;
    try {
      await action();
      return true;
    } catch (e) {
      error = errorMessage(e);
      return false;
    }
  }

  function commit(): Promise<boolean> {
    const next = value.trim();
    if (next === entry.sayAs.value) return Promise.resolve(true);
    return run(() =>
      book.savePronunciation(scope, { ...entry, sayAs: { ...entry.sayAs, value: next } }),
    );
  }

  function setPhonetic(phonetic: boolean) {
    void run(() =>
      book.savePronunciation(scope, {
        ...entry,
        sayAs: { kind: phonetic ? "phonemes" : "text", value: entry.sayAs.value },
      }),
    );
  }

  function moveTo(target: PronScope) {
    void run(() => book.movePronunciation(entry.id, target));
  }

  async function test() {
    if (await commit()) await attempt(api.testPronunciation(book.id, entry.word));
  }
</script>

<li class="space-y-2 px-4 py-3">
  <div class="flex flex-wrap items-center gap-3">
    <span class="w-28 shrink-0 truncate font-medium" title={entry.word}>{entry.word}</span>
    <span class="text-muted-foreground">→</span>
    <Input
      bind:value
      placeholder="Sounds like"
      aria-label={`How “${entry.word}” sounds`}
      class="h-8 min-w-40 flex-1"
      disabled={book.locked}
      onchange={commit}
    />
    <Button variant="ghost" size="icon-xs" aria-label={`Hear “${entry.word}”`} onclick={test}>
      <Play />
    </Button>
    <ToggleGroup.Root
      type="single"
      variant="outline"
      size="sm"
      value={scope}
      disabled={book.locked}
      onValueChange={(v) => v && v !== scope && moveTo(v as PronScope)}
    >
      <ToggleGroup.Item value="book">This book</ToggleGroup.Item>
      <ToggleGroup.Item value="global">All books</ToggleGroup.Item>
    </ToggleGroup.Root>
    <Button
      variant="ghost"
      size="icon-xs"
      aria-label={`Delete “${entry.word}”`}
      disabled={book.locked}
      onclick={() => run(() => book.removePronunciation(scope, entry.id))}
    >
      <Trash />
    </Button>
  </div>
  <details class="text-xs text-muted-foreground">
    <summary class="cursor-pointer select-none">Advanced</summary>
    <label class="mt-2 flex items-center gap-2">
      <Switch
        size="sm"
        disabled={book.locked}
        bind:checked={() => entry.sayAs.kind === "phonemes", setPhonetic}
      />
      Use phonetic spelling
    </label>
  </details>
  {#if error}<p class="text-xs text-destructive">{error}</p>{/if}
</li>
