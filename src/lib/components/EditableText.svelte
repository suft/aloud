<script lang="ts">
  import { cn } from "$lib/utils";

  let {
    value,
    onsave,
    placeholder = "",
    disabled = false,
    class: className,
  }: {
    value: string;
    onsave: (value: string) => void;
    placeholder?: string;
    disabled?: boolean;
    class?: string;
  } = $props();

  let editing = $state(false);
  let draft = $state("");

  function start() {
    draft = value;
    editing = true;
  }

  function commit() {
    if (!editing) return;
    editing = false;
    const next = draft.trim();
    if (next !== value.trim()) onsave(next);
  }

  function focus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

{#if editing}
  <input
    use:focus
    bind:value={draft}
    {placeholder}
    class={cn(
      "-mx-1 [field-sizing:content] max-w-full min-w-24 rounded-md bg-background/70 px-1 ring-2 ring-ring/50 outline-none",
      className,
    )}
    onblur={commit}
    onkeydown={(e) => {
      if (e.key === "Enter") e.currentTarget.blur();
      else if (e.key === "Escape") editing = false;
    }}
  />
{:else}
  <button
    type="button"
    {disabled}
    class={cn(
      "-mx-1 max-w-full cursor-text rounded-md px-1 text-left hover:bg-foreground/5 disabled:cursor-default disabled:hover:bg-transparent",
      !value && "text-muted-foreground",
      className,
    )}
    onclick={start}
  >
    {value || placeholder}
  </button>
{/if}
