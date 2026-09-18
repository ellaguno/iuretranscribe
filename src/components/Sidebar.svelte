<script lang="ts">
  import { app, selectedModel, type View } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";

  const items: { id: View; label: string; icon: string }[] = [
    { id: "transcribe", label: "Transcribir", icon: "waveform" },
    { id: "models", label: "Modelos", icon: "layers" },
    { id: "settings", label: "Ajustes", icon: "settings" },
  ];
  let model = $derived(selectedModel());
  let active = $derived(app.jobs.filter((j) => j.status !== "done" && j.status !== "error" && j.status !== "cancelled").length);
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="logo"><Icon name="waveform" size={20} stroke={2.5} /></div>
    <div>
      <div class="name">IureTranscribe</div>
      <div class="ver">v{app.sys?.version ?? ""}</div>
    </div>
  </div>

  <nav>
    {#each items as it}
      <button class:active={app.view === it.id} onclick={() => (app.view = it.id)}>
        <Icon name={it.icon} />
        <span>{it.label}</span>
        {#if it.id === "transcribe" && active > 0}
          <span class="badge">{active}</span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="foot">
    <div class="row">
      <Icon name="cpu" size={15} />
      <span>{app.sys?.backend ?? "…"}</span>
      {#if app.sys?.backend === "CPU" && app.settings?.useGpu}
        <span class="hint" title="Este binario se compiló sin soporte de GPU">sin GPU</span>
      {/if}
    </div>
    <div class="row">
      <Icon name="layers" size={15} />
      {#if model}
        <span class:warn={!model.downloaded}>{model.name}{model.downloaded ? "" : " (no descargado)"}</span>
      {:else}
        <span>Sin modelo</span>
      {/if}
    </div>
  </div>
</aside>

<style>
  .sidebar { display: flex; flex-direction: column; background: var(--surface); border-right: 1px solid var(--border); padding: 18px 12px; gap: 16px; }
  .brand { display: flex; align-items: center; gap: 10px; padding: 4px 8px; }
  .logo { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; color: #fff; background: linear-gradient(135deg, #1e3a8a, #0f766e); }
  .name { font-weight: 700; font-size: 15px; letter-spacing: -0.01em; }
  .ver { font-size: 11.5px; color: var(--muted); }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav button { display: flex; align-items: center; gap: 10px; padding: 9px 10px; border-radius: 9px; color: var(--text-2); font-weight: 550; text-align: left; }
  nav button:hover { background: var(--surface-2); }
  nav button.active { background: var(--accent-soft); color: var(--accent); }
  .badge { margin-left: auto; font-size: 11px; background: var(--accent); color: var(--accent-text); border-radius: 999px; padding: 1px 7px; }
  .foot { margin-top: auto; display: flex; flex-direction: column; gap: 6px; padding: 10px; border-top: 1px solid var(--border); font-size: 12.5px; color: var(--muted); }
  .row { display: flex; align-items: center; gap: 7px; overflow: hidden; }
  .row span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .warn { color: var(--warn); }
</style>
