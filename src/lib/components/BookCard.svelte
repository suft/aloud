<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import type { ProjectSummary } from "$lib/api";
  import { buttonVariants } from "$lib/components/ui/button";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import { percent } from "$lib/format";
  import { app } from "$lib/stores/app.svelte";
  import { render } from "$lib/stores/render.svelte";
  import { attempt } from "$lib/toast";
  import { cn } from "$lib/utils";
  import Cover from "./Cover.svelte";

  let { book, onremove }: { book: ProjectSummary; onremove: () => void } = $props();

  const status = $derived(book.status);
  const progress = $derived(render.progress[book.id]?.fraction ?? book.progress);
  const showBar = $derived(
    progress > 0 &&
      (status.state === "rendering" ||
        status.state === "exporting" ||
        status.state === "queued" ||
        status.state === "paused"),
  );
  const queuePosition = $derived(render.position(book.id));

  function open() {
    app.go({ name: "book", id: book.id });
  }
</script>

<div class="group relative transition-transform duration-200 hover:scale-[1.02]">
  <button type="button" class="block w-full text-left" onclick={open}>
    <div
      class="relative aspect-[2/3] overflow-hidden rounded-xl border border-border/40 shadow-md transition-shadow duration-200 group-hover:shadow-xl"
    >
      <Cover path={book.coverPath} title={book.title} accent={book.accent} />

      {#if status.state !== "draft"}
        <span
          class={cn(
            "absolute bottom-3 left-2 flex items-center gap-1 rounded-full bg-black/60 px-2 py-0.5 text-xs font-medium text-white backdrop-blur",
            status.state === "done" && "bg-emerald-600/90",
            status.state === "failed" && "bg-destructive/90",
          )}
        >
          {#if status.state === "done"}
            <Check class="size-3" /> Ready
          {:else if status.state === "rendering" || status.state === "exporting"}
            Making… {percent(progress)}%
          {:else if status.state === "queued"}
            Queued{queuePosition >= 0 ? ` #${queuePosition + 1}` : ""}
          {:else if status.state === "paused"}
            Paused
          {:else}
            Problem
          {/if}
        </span>
      {/if}

      {#if showBar}
        <div class="absolute inset-x-0 bottom-0 h-1 bg-black/30">
          <div
            class="h-full transition-[width] duration-200"
            style:width="{percent(progress)}%"
            style:background-color={book.accent ?? "var(--primary)"}
          ></div>
        </div>
      {/if}
    </div>
    <div class="mt-3 space-y-0.5 px-0.5">
      <div class="line-clamp-2 text-sm leading-snug font-medium">{book.title}</div>
      <div class="truncate text-xs text-muted-foreground">{book.authors.join(", ")}</div>
    </div>
  </button>

  <div class="absolute top-2 right-2">
    <DropdownMenu.Root>
      <DropdownMenu.Trigger
        aria-label="More"
        class={cn(
          buttonVariants({ variant: "secondary", size: "icon-sm" }),
          "opacity-0 shadow-sm transition-opacity group-hover:opacity-100 focus-visible:opacity-100 data-[state=open]:opacity-100",
        )}
      >
        <Ellipsis />
      </DropdownMenu.Trigger>
      <DropdownMenu.Content align="end" class="w-56">
        <DropdownMenu.Item onSelect={open}>Open</DropdownMenu.Item>
        {#if status.state === "done"}
          <DropdownMenu.Item onSelect={() => attempt(revealItemInDir(status.output))}>
            Show audiobook in folder
          </DropdownMenu.Item>
        {/if}
        <DropdownMenu.Separator />
        <DropdownMenu.Item variant="destructive" onSelect={onremove}>Remove</DropdownMenu.Item>
      </DropdownMenu.Content>
    </DropdownMenu.Root>
  </div>
</div>
