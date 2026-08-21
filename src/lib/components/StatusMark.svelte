<script lang="ts">
  import Icon from './Icon.svelte';
  import type { ReceiptStatus } from '../types';

  export let status: ReceiptStatus;
  export let label: string;
</script>

<span class:running={status === 'running'} class:failed={status === 'failed'} class:blocked={status === 'blocked'} class="status-mark">
  {#if status === 'completed'}<Icon name="check" size={13} />
  {:else if status === 'failed' || status === 'blocked'}<Icon name={status === 'blocked' ? 'shield' : 'alert'} size={13} />
  {:else}<span class="status-dot"></span>{/if}
  {label}
</span>
