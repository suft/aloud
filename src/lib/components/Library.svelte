<script lang="ts">
  import BookAudio from "@lucide/svelte/icons/book-audio";
  import Plus from "@lucide/svelte/icons/plus";
  import { open } from "@tauri-apps/plugin-dialog";
  import { fade } from "svelte/transition";
  import { api, type ProjectSummary } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { dur } from "$lib/motion";
  import { projects } from "$lib/stores/projects.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { attempt } from "$lib/toast";
  import BookCard from "./BookCard.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let removing = $state<ProjectSummary | null>(null);
  let confirmOpen = $state(false);

  async function choose() {
    const picked = await attempt(
      open({ multiple: true, filters: [{ name: "EPUB", extensions: ["epub"] }] }),
    );
    if (picked) await projects.importPaths(picked);
  }

  async function remove() {
    if (!removing) return;
    await attempt(api.deleteProject(removing.id));
    removing = null;
    await projects.refresh();
  }
</script>

<div class="mx-auto max-w-6xl space-y-6 p-8">
  {#if render.queue.paused && render.queue.items.length > 0}
    <div
      class="flex items-center justify-between gap-4 rounded-2xl border border-border/60 bg-card/80 px-5 py-3 text-sm shadow-sm backdrop-blur"
      transition:fade={{ duration: dur(150) }}
    >
      <span>
        {render.restarted
          ? "Aloud was closed while making audiobooks."
          : "Making audiobooks is paused."}
      </span>
      <Button size="sm" class="rounded-full" onclick={() => render.resume()}>Resume</Button>
    </div>
  {/if}

  {#if projects.loaded && projects.list.length === 0}
    <div
      class="flex min-h-[420px] flex-col items-center justify-center gap-4 rounded-3xl border-2 border-dashed border-border p-10 text-center"
      in:fade={{ duration: dur(200) }}
    >
      <BookAudio class="size-16 text-muted-foreground" strokeWidth={1.25} />
      <div class="text-xl font-semibold tracking-tight">Drop an EPUB here</div>
      <Button size="lg" class="rounded-full px-5" onclick={choose}>Choose a book…</Button>
      <p class="text-xs text-muted-foreground">DRM-free EPUB files work best</p>
    </div>
  {:else if projects.loaded}
    <div class="flex items-center justify-between">
      <h1 class="text-2xl font-semibold tracking-tight">Your books</h1>
      <Button class="rounded-full" onclick={choose}><Plus /> Add book</Button>
    </div>
    <div
      class="grid grid-cols-[repeat(auto-fill,minmax(168px,1fr))] gap-6"
      in:fade={{ duration: dur(200) }}
    >
      {#each projects.list as book (book.id)}
        <BookCard
          {book}
          onremove={() => {
            removing = book;
            confirmOpen = true;
          }}
        />
      {/each}
    </div>
  {/if}
</div>

<ConfirmDialog
  bind:open={confirmOpen}
  title="Remove book"
  description={removing?.damaged
    ? "Remove this damaged book from Aloud? Its saved data is deleted; your exported audiobook file is kept."
    : `Remove “${removing?.title ?? ""}” from Aloud? Your exported audiobook file is kept.`}
  confirmLabel="Remove"
  destructive
  onconfirm={remove}
/>
