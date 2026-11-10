<script lang="ts">
  import { imageSrc } from "$lib/api";
  import { cn } from "$lib/utils";

  let {
    path,
    title,
    accent,
    version,
    showTitle = true,
    class: className,
  }: {
    path: string | null;
    title: string;
    accent: string | null;
    version?: number;
    showTitle?: boolean;
    class?: string;
  } = $props();

  const base = $derived(accent ?? "var(--primary)");
</script>

{#if path}
  <img
    src={imageSrc(path, version)}
    alt=""
    crossorigin="anonymous"
    draggable="false"
    class={cn("size-full object-cover", className)}
  />
{:else}
  <div
    class={cn("flex size-full items-end p-3", className)}
    style:background="linear-gradient(160deg, color-mix(in oklch, {base} 85%, white), color-mix(in oklch,
    {base} 55%, black))"
  >
    {#if showTitle}
      <span class="line-clamp-4 font-serif text-base leading-tight font-semibold text-white">
        {title}
      </span>
    {/if}
  </div>
{/if}
