<script lang="ts">
  import type { JobMeta } from "../lib/state.svelte";
  import { t } from "../lib/i18n.svelte";

  let { meta = $bindable(), hint = "" }: { meta: JobMeta; hint?: string } = $props();
  const uid = Math.random().toString(36).slice(2, 7);
</script>

<div class="meta-form">
  <div class="field">
    <label for="m-date-{uid}">{t("meta.date")}</label>
    <input id="m-date-{uid}" class="input" placeholder={t("meta.datePh")} bind:value={meta.date} />
  </div>
  <div class="field">
    <label for="m-place-{uid}">{t("meta.place")}</label>
    <input id="m-place-{uid}" class="input" placeholder={t("meta.placePh")} bind:value={meta.place} />
  </div>
  <div class="field wide">
    <label for="m-people-{uid}">{t("meta.participants")}</label>
    <textarea id="m-people-{uid}" class="input short" placeholder={t("meta.participantsPh")} bind:value={meta.participants}></textarea>
  </div>
  <div class="field wide">
    <label for="m-notes-{uid}">{t("meta.notes")}</label>
    <textarea id="m-notes-{uid}" class="input short" placeholder={t("meta.notesPh")} bind:value={meta.notes}></textarea>
  </div>
  {#if hint}<p class="hint wide">{hint}</p>{/if}
</div>

<style>
  .meta-form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 14px; }
  .meta-form .wide { grid-column: 1 / -1; }
  .meta-form textarea.short { min-height: 56px; }
</style>
