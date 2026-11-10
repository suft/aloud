<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import { Button } from "$lib/components/ui/button";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import { Switch } from "$lib/components/ui/switch";
  import { Textarea } from "$lib/components/ui/textarea";
  import { formatBytes, parseAuthors } from "$lib/format";
  import type { BookView } from "$lib/stores/book.svelte";
  import { projects } from "$lib/stores/projects.svelte";
  import { attempt } from "$lib/toast";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Cover from "./Cover.svelte";

  let { book }: { book: BookView } = $props();

  let cacheSize = $state<number | null>(null);
  let clearOpen = $state(false);

  const detail = $derived(book.detail!);
  const project = $derived(detail.project);
  const status = $derived(project.status.state);

  async function loadCacheSize() {
    cacheSize = (await attempt(api.projectCacheSize(book.id))) ?? null;
  }

  $effect(() => {
    void status;
    void loadCacheSize();
  });

  async function chooseCover() {
    const path = await attempt(
      open({ filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "webp"] }] }),
    );
    if (!path) return;
    await book.setCover(path);
    await projects.refresh();
  }

  async function clearCache() {
    await attempt(api.clearProjectCache(book.id));
    await Promise.all([book.load(), loadCacheSize()]);
  }
</script>

<div class="space-y-6 rounded-2xl border border-border/60 bg-card/80 p-6 shadow-sm backdrop-blur">
  {#if book.locked}
    <p class="text-xs text-muted-foreground">Pause first to change this.</p>
  {/if}

  <div class="flex gap-6">
    <div class="w-32 shrink-0 space-y-3">
      <div class="aspect-[2/3] overflow-hidden rounded-xl shadow-md">
        <Cover
          path={detail.coverPath}
          title={project.meta.title}
          accent={project.meta.accent}
          version={book.coverVersion}
        />
      </div>
      <Button
        variant="outline"
        size="sm"
        class="w-full"
        disabled={book.locked}
        onclick={chooseCover}
      >
        Change cover…
      </Button>
    </div>

    <div class="min-w-0 flex-1 space-y-4">
      <div class="space-y-1.5">
        <Label for="details-title">Title</Label>
        <Input
          id="details-title"
          value={project.meta.title}
          disabled={book.locked}
          onchange={(e) =>
            e.currentTarget.value.trim() &&
            book.patch({ meta: { title: e.currentTarget.value.trim() } })}
        />
      </div>
      <div class="space-y-1.5">
        <Label for="details-authors">Authors</Label>
        <Input
          id="details-authors"
          value={project.meta.authors.join(", ")}
          placeholder="Separate several authors with commas"
          disabled={book.locked}
          onchange={(e) => book.patch({ meta: { authors: parseAuthors(e.currentTarget.value) } })}
        />
      </div>
      <div class="space-y-1.5">
        <Label for="details-year">Year</Label>
        <Input
          id="details-year"
          value={project.meta.date ?? ""}
          class="w-40"
          disabled={book.locked}
          onchange={(e) => book.patch({ meta: { date: e.currentTarget.value.trim() || null } })}
        />
      </div>
      <div class="space-y-1.5">
        <Label for="details-description">Description</Label>
        <Textarea
          id="details-description"
          value={project.meta.description ?? ""}
          rows={4}
          disabled={book.locked}
          onchange={(e) =>
            book.patch({ meta: { description: e.currentTarget.value.trim() || null } })}
        />
      </div>
    </div>
  </div>

  <div class="flex items-center justify-between gap-4 border-t border-border/60 pt-5">
    <Label for="details-intro" class="cursor-pointer">Spoken introduction (title and author)</Label>
    <Switch
      id="details-intro"
      disabled={book.locked}
      bind:checked={() => project.intro, (intro) => book.patch({ intro })}
    />
  </div>

  <div class="space-y-1.5">
    <Label for="details-file">Audiobook file name</Label>
    <Input id="details-file" value={detail.outputFileName} readonly />
  </div>

  <div class="flex items-center justify-between gap-4">
    <div class="text-sm">
      <div class="font-medium">Saved audio for this book</div>
      <div class="text-muted-foreground">{cacheSize === null ? "…" : formatBytes(cacheSize)}</div>
    </div>
    {#if status === "done"}
      <Button variant="outline" size="sm" onclick={() => (clearOpen = true)}>Free up space</Button>
    {/if}
  </div>
</div>

<ConfirmDialog
  bind:open={clearOpen}
  title="Free up space"
  description="Remove saved audio for this book? You can still play the exported audiobook, but changes will need a full redo"
  confirmLabel="Remove"
  destructive
  onconfirm={clearCache}
/>
