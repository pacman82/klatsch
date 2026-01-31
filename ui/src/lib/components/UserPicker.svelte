<script lang="ts">
  import { onDestroy } from 'svelte';
  import { user } from '$lib/stores/user';

  let name = '';
  const unsub = user.subscribe((v) => (name = v));
  onDestroy(() => unsub());

  function save() {
    const trimmed = name.trim().slice(0, 32);
    if (trimmed) user.set(trimmed);
  }
</script>

<div class="user-picker">
  <label for="username">You</label>
  <input id="username" bind:value={name} on:blur={save} maxlength="32" />
  <button type="button" on:click={save}>Save</button>
</div>

<style>
  .user-picker {
    max-width: 600px;
    margin: 0.5rem auto;
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  input { flex: 1; padding: 0.35rem; border-radius: 6px; border: 1px solid #ccc }
  button { padding: 0.35rem 0.6rem; border-radius: 6px; background: #6366f1; color: white; border: none }
</style>
