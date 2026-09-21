<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, selectedModel, toast, type View } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import iconUrl from "../assets/icon.png";

  const links = [
    { label: "Sitio web", url: "https://iurefficient.com", icon: "globe" },
    { label: "Demo", url: "https://demo.iurefficient.com", icon: "demo" },
    { label: "YouTube", url: "https://youtube.com/@iurefficient", icon: "youtube" },
  ];
  function go(url: string) {
    openUrl(url).catch((e) => toast(`No se pudo abrir ${url}: ${e}`, "error"));
  }

  const items: { id: View; label: string; icon: string }[] = [
    { id: "transcribe", label: "Transcribir", icon: "waveform" },
    { id: "record", label: "Grabar", icon: "mic" },
    { id: "models", label: "Modelos", icon: "layers" },
    { id: "settings", label: "Ajustes", icon: "settings" },
  ];
  let model = $derived(selectedModel());
  let active = $derived(app.jobs.filter((j) => j.status !== "done" && j.status !== "error" && j.status !== "cancelled").length);
</script>

<aside class="sidebar">
  <div class="brand">
    <img class="logo" src={iconUrl} alt="" width="40" height="40" />
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
        {#if it.id === "record" && app.recording.active}
          <span class="recdot" title="Grabando"></span>
        {/if}
      </button>
    {/each}
  </nav>

  <div class="links">
    <div class="links-title">Iurefficient</div>
    {#each links as l}
      <button onclick={() => go(l.url)} title={l.url}>
        <Icon name={l.icon} size={15} />
        <span>{l.label}</span>
        <Icon name="external" size={12} />
      </button>
    {/each}
  </div>

  <div class="foot">
    <button class="row conn" class:ok={app.iureSession?.loggedIn} onclick={() => (app.view = "settings")} title={app.iureSession?.loggedIn ? "Conectado a Iurefficient" : "Conectar con Iurefficient"}>
      <Icon name="cloud" size={15} />
      <span>{app.iureSession?.loggedIn ? `Iurefficient: ${app.iureSession.name ?? "conectado"}` : "Conectar con Iurefficient"}</span>
    </button>
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
  .logo { width: 40px; height: 40px; border-radius: 10px; flex-shrink: 0; }
  .name { font-weight: 700; font-size: 15px; letter-spacing: -0.01em; }
  .ver { font-size: 11.5px; color: var(--muted); }
  nav { display: flex; flex-direction: column; gap: 2px; }
  nav button { display: flex; align-items: center; gap: 10px; padding: 9px 10px; border-radius: 9px; color: var(--text-2); font-weight: 550; text-align: left; }
  nav button:hover { background: var(--surface-2); }
  nav button.active { background: var(--accent-soft); color: var(--accent); }
  .recdot { margin-left: auto; width: 9px; height: 9px; border-radius: 50%; background: var(--danger); animation: blink 1.2s ease-in-out infinite; }
  @keyframes blink { 50% { opacity: 0.25; } }
  .badge { margin-left: auto; font-size: 11px; background: var(--accent); color: var(--accent-text); border-radius: 999px; padding: 1px 7px; }
  .links { margin-top: auto; display: flex; flex-direction: column; gap: 1px; padding: 6px 0; }
  .links-title { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--muted); padding: 4px 10px 6px; }
  .links button { display: flex; align-items: center; gap: 9px; padding: 7px 10px; border-radius: 8px; color: var(--text-2); font-size: 13px; text-align: left; }
  .links button :global(svg:last-child) { margin-left: auto; opacity: 0.5; }
  .links button:hover { background: var(--surface-2); color: var(--accent); }
  .foot { display: flex; flex-direction: column; gap: 6px; padding: 10px; border-top: 1px solid var(--border); font-size: 12.5px; color: var(--muted); }
  .row { display: flex; align-items: center; gap: 7px; overflow: hidden; }
  .row span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .warn { color: var(--warn); }
  .conn { text-align: left; color: var(--accent); font-weight: 600; padding: 4px 0; }
  .conn.ok { color: var(--success); font-weight: 550; }
  .conn:hover { text-decoration: underline; }
</style>
