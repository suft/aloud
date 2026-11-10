<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import * as Dialog from "$lib/components/ui/dialog";

  let {
    open = $bindable(false),
    title,
    description,
    confirmLabel,
    cancelLabel = "Cancel",
    destructive = false,
    onconfirm,
  }: {
    open?: boolean;
    title: string;
    description: string;
    confirmLabel: string;
    cancelLabel?: string;
    destructive?: boolean;
    onconfirm: () => void;
  } = $props();
</script>

<Dialog.Root bind:open>
  <Dialog.Content showCloseButton={false}>
    <Dialog.Header>
      <Dialog.Title>{title}</Dialog.Title>
      <Dialog.Description>{description}</Dialog.Description>
    </Dialog.Header>
    <Dialog.Footer>
      <Button variant="ghost" onclick={() => (open = false)}>{cancelLabel}</Button>
      <Button
        variant={destructive ? "destructive" : "default"}
        onclick={() => {
          open = false;
          onconfirm();
        }}
      >
        {confirmLabel}
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
