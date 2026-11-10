<script lang="ts">
  import { fade } from "svelte/transition";
  import { on } from "$lib/api";
  import { Skeleton } from "$lib/components/ui/skeleton";
  import * as Tabs from "$lib/components/ui/tabs";
  import { dur } from "$lib/motion";
  import { BookView } from "$lib/stores/book.svelte";
  import BookHero from "./BookHero.svelte";
  import ChaptersTab from "./ChaptersTab.svelte";
  import DetailsTab from "./DetailsTab.svelte";
  import ProgressCard from "./ProgressCard.svelte";
  import PronunciationsTab from "./PronunciationsTab.svelte";

  let { id }: { id: string } = $props();

  const book = $derived(new BookView(id));
  const project = $derived(book.detail?.project);

  $effect(() => {
    const view = book;
    void view.load();
    const subscription = on.projectUpdated((p) => {
      if (p.id === view.id) void view.load();
    });
    return () => void subscription.then((unlisten) => unlisten());
  });
</script>

{#if project}
  <div
    style:--book-accent={project.meta.accent ?? "var(--primary)"}
    in:fade={{ duration: dur(200) }}
  >
    <BookHero {book} />

    <div class="mx-auto max-w-4xl space-y-6 px-8 pb-10">
      {#if project.status.state === "failed"}
        <div class="rounded-2xl bg-destructive/10 p-5 text-sm text-destructive">
          <div class="font-medium">Something went wrong</div>
          <p class="mt-1">{project.status.message}</p>
        </div>
      {/if}

      <ProgressCard {book} />

      <Tabs.Root value="chapters">
        <Tabs.List>
          <Tabs.Trigger value="chapters">Chapters</Tabs.Trigger>
          <Tabs.Trigger value="pronunciations">Pronunciations</Tabs.Trigger>
          <Tabs.Trigger value="details">Details</Tabs.Trigger>
        </Tabs.List>
        <Tabs.Content value="chapters" class="pt-4"><ChaptersTab {book} /></Tabs.Content>
        <Tabs.Content value="pronunciations" class="pt-4"><PronunciationsTab {book} /></Tabs.Content
        >
        <Tabs.Content value="details" class="pt-4"><DetailsTab {book} /></Tabs.Content>
      </Tabs.Root>
    </div>
  </div>
{:else}
  <div class="mx-auto max-w-4xl space-y-4 p-8">
    <Skeleton class="h-[280px] w-full rounded-2xl" />
  </div>
{/if}
