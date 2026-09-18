<script lang="ts">
  import type { JobMeta } from "../lib/state.svelte";

  let { meta = $bindable(), hint = "" }: { meta: JobMeta; hint?: string } = $props();
  const uid = Math.random().toString(36).slice(2, 7);
</script>

<div class="meta-form">
  <div class="field">
    <label for="m-date-{uid}">Fecha</label>
    <input id="m-date-{uid}" class="input" placeholder="p. ej. 18 de septiembre de 2026, 10:00" bind:value={meta.date} />
  </div>
  <div class="field">
    <label for="m-place-{uid}">Lugar</label>
    <input id="m-place-{uid}" class="input" placeholder="p. ej. Sala de juntas / videollamada" bind:value={meta.place} />
  </div>
  <div class="field wide">
    <label for="m-people-{uid}">Participantes</label>
    <textarea id="m-people-{uid}" class="input short" placeholder="Un nombre por línea, con cargo o rol si aplica" bind:value={meta.participants}></textarea>
  </div>
  <div class="field wide">
    <label for="m-notes-{uid}">Notas adicionales</label>
    <textarea id="m-notes-{uid}" class="input short" placeholder="Contexto útil para la minuta: asunto, cliente, expediente, acuerdos previos…" bind:value={meta.notes}></textarea>
  </div>
  {#if hint}<p class="hint wide">{hint}</p>{/if}
</div>

<style>
  .meta-form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 14px; }
  .meta-form .wide { grid-column: 1 / -1; }
  .meta-form textarea.short { min-height: 56px; }
</style>
