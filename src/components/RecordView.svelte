<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { app, loadDevices, saveSettings, selectedModel, startRecording, stopRecording, toast } from "../lib/state.svelte";
  import { fmtDuration, fmtTimestamp } from "../lib/format";
  import Icon from "./Icon.svelte";
  import MetaForm from "./MetaForm.svelte";

  let s = $derived(app.settings!);
  let rec = $derived(app.recording);
  let devices = $derived(app.devices);
  let sysUnavailable = $derived(devices?.systemCapture === "unavailable");
  let canRecord = $derived((s.recordMic || (s.recordSystem && !sysUnavailable)) && !app.recordingBusy);
  let isLinux = $derived(app.sys?.platform === "linux");
  let model = $derived(selectedModel());
  let liveEl = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (liveEl && app.liveSegments.length) liveEl.scrollTop = liveEl.scrollHeight;
  });

  // Paneles plegables: se pliegan solos al iniciar la grabación y se abren al detenerla.
  let openSources = $state(true);
  let openMeta = $state(true);
  let wasActive = false;
  $effect(() => {
    const active = rec.active;
    if (active && !wasActive) {
      openSources = false;
      openMeta = false;
    } else if (!active && wasActive) {
      openSources = true;
      openMeta = true;
    }
    wasActive = active;
  });

  onMount(() => {
    if (!app.devices) loadDevices();
  });

  async function pickDir() {
    const dir = await open({ directory: true, title: "Carpeta de grabaciones" });
    if (typeof dir === "string") {
      await saveSettings({ recordingsDir: dir });
      app.sys = await api.systemInfo();
    }
  }
  function openDir() {
    if (app.sys) api.openPath(app.sys.recordingsDir).catch((e) => toast(String(e), "error"));
  }
</script>

<header class="top">
  <div>
    <h1>Grabar</h1>
    <p class="hint">Graba el micrófono y lo que suena en la bocina (videollamadas, reuniones) y transcribe al terminar.</p>
  </div>
</header>

<div class="content scroll">
  <div class="card rec" class:live={rec.active}>
    <div class="rec-main">
      <button class="rec-btn" class:stop={rec.active} disabled={!canRecord && !rec.active} onclick={() => (rec.active ? stopRecording() : startRecording())} aria-label={rec.active ? "Detener" : "Grabar"}>
        {#if app.recordingBusy}
          <span class="spin"><Icon name="loader" size={30} /></span>
        {:else if rec.active}
          <Icon name="stopfill" size={30} />
        {:else}
          <Icon name="dot" size={30} />
        {/if}
      </button>
      <div class="rec-info">
        {#if rec.active}
          <div class="timer">{fmtDuration(rec.elapsedSecs)}</div>
          <div class="hint">
            Grabando… pulsa para detener{s.autoTranscribeRecording ? " y transcribir" : ""}.
            {#if rec.live}<span class="livetag"><Icon name="zap" size={12} /> en vivo{rec.livePendingSecs > 3 ? ` · retraso ${Math.round(rec.livePendingSecs)} s` : ""}</span>{/if}
          </div>
        {:else}
          <div class="timer idle">00:00</div>
          <div class="hint">{canRecord ? "Pulsa para iniciar la grabación." : "Activa al menos una fuente para grabar."}</div>
        {/if}
        {#if rec.error}<div class="err"><Icon name="alert" size={14} /> {rec.error}</div>{/if}
      </div>
    </div>
    <div class="meters">
      <div class="meter" class:off={!s.recordMic}>
        <span><Icon name="mic" size={14} /> Micrófono</span>
        <div class="bar"><div style="width:{Math.round(rec.micLevel * 100)}%"></div></div>
      </div>
      <div class="meter" class:off={!s.recordSystem || sysUnavailable}>
        <span><Icon name="speaker" size={14} /> Sistema</span>
        <div class="bar"><div style="width:{Math.round(rec.sysLevel * 100)}%"></div></div>
      </div>
    </div>
  </div>

  <div class="grid">
    <section class="card panel" class:collapsed={!openSources}>
      <button class="phead" onclick={() => (openSources = !openSources)} aria-expanded={openSources}>
        <h2><Icon name="settings" size={16} /> Fuentes</h2>
        {#if !openSources}
          <span class="summary hint">
            {[s.recordMic ? "micrófono" : "", s.recordSystem && !sysUnavailable ? "sistema" : ""].filter(Boolean).join(" + ") || "sin fuentes"}{s.liveTranscription ? (s.liveIsFinal ? " · en vivo (final)" : " · en vivo (vista previa)") : ""}{(!s.liveTranscription || !s.liveIsFinal) && s.autoTranscribeRecording ? " · transcribe al detener" : ""}
          </span>
        {/if}
        <span class="chev" class:up={openSources}><Icon name="chevron" size={14} /></span>
      </button>
      {#if openSources}
      <div class="pbody">
      <div class="switchrow">
        <div>
          <span class="label">Micrófono</span>
          {#if !isLinux && devices && devices.inputs.length}
            <select class="input small" value={s.micDevice ?? "default"} disabled={rec.active} onchange={(e) => saveSettings({ micDevice: (e.target as HTMLSelectElement).value === "default" ? null : (e.target as HTMLSelectElement).value })}>
              <option value="default">Predeterminado del sistema</option>
              {#each devices.inputs as d}<option value={d.id}>{d.name}{d.isDefault ? " (predeterminado)" : ""}</option>{/each}
            </select>
          {:else}
            <p class="hint">Se usa el micrófono predeterminado del sistema.</p>
          {/if}
        </div>
        <button class="switch" class:on={s.recordMic} aria-label="Grabar micrófono" disabled={rec.active} onclick={() => saveSettings({ recordMic: !s.recordMic })}></button>
      </div>
      <div class="switchrow">
        <div>
          <span class="label">Audio del sistema (bocina)</span>
          <p class="hint">{devices?.note ?? "…"}</p>
        </div>
        <button class="switch" class:on={s.recordSystem && !sysUnavailable} aria-label="Grabar audio del sistema" disabled={rec.active || sysUnavailable} onclick={() => saveSettings({ recordSystem: !s.recordSystem })}></button>
      </div>
      <div class="switchrow">
        <div>
          <span class="label">Distinguir quién habla (tú / interlocutor)</span>
          <p class="hint">Con micrófono y sistema activos, cada fuente se transcribe por separado y los segmentos llevan nombre. Tu nombre:</p>
          <input class="input small" placeholder={app.iureSession?.name ?? "Tu nombre"} value={s.myName} disabled={rec.active} oninput={(e) => saveSettings({ myName: (e.target as HTMLInputElement).value })} />
        </div>
        <button class="switch" class:on={s.speakerSplit && s.recordMic && s.recordSystem} aria-label="Quién habla" disabled={rec.active || !s.recordMic || !s.recordSystem} onclick={() => saveSettings({ speakerSplit: !s.speakerSplit })}></button>
      </div>
      <div class="switchrow">
        <div>
          <span class="label">Transcribir en vivo mientras grabo</span>
          <p class="hint">
            Procesa el audio en bloques de {Math.round(s.liveChunkSecs)} s con el modelo {model?.name ?? "seleccionado"}{model && !model.downloaded ? " (no descargado)" : ""}.
            {app.sys?.backend === "CPU" ? "Sin GPU conviene un modelo pequeño (Small o Turbo Q5) para que no se rezague." : ""}
          </p>
          <select class="input small" value={String(Math.round(s.liveChunkSecs))} disabled={rec.active || !s.liveTranscription} onchange={(e) => saveSettings({ liveChunkSecs: Number((e.target as HTMLSelectElement).value) })}>
            <option value="5">Bloques de 5 s (más inmediato)</option>
            <option value="8">Bloques de 8 s (recomendado)</option>
            <option value="12">Bloques de 12 s</option>
            <option value="20">Bloques de 20 s (mejor contexto)</option>
          </select>
        </div>
        <button class="switch" class:on={s.liveTranscription} aria-label="Transcribir en vivo" disabled={rec.active} onclick={() => saveSettings({ liveTranscription: !s.liveTranscription })}></button>
      </div>
      {#if s.liveTranscription}
        <div class="switchrow">
          <div>
            <span class="label">Usar la transcripción en vivo como resultado final</span>
            <p class="hint">Al detener se generan los archivos, el resumen y la minuta con lo ya transcrito, sin repetir el trabajo. Siempre podrás pedir una segunda pasada con calidad alta desde el panel del archivo.</p>
          </div>
          <button class="switch" class:on={s.liveIsFinal} aria-label="Usar en vivo como final" disabled={rec.active} onclick={() => saveSettings({ liveIsFinal: !s.liveIsFinal })}></button>
        </div>
      {/if}
      {#if !s.liveTranscription || !s.liveIsFinal}
        <div class="switchrow">
          <div>
            <span class="label">Transcribir la grabación completa al detener</span>
            <p class="hint">Se agrega a la cola y se procesa con el modelo seleccionado y calidad alta.</p>
          </div>
          <button class="switch" class:on={s.autoTranscribeRecording} aria-label="Transcribir automáticamente" onclick={() => saveSettings({ autoTranscribeRecording: !s.autoTranscribeRecording })}></button>
        </div>
      {/if}
      <div class="dir">
        <span class="label">Carpeta de grabaciones</span>
        <div class="row">
          <span class="path" title={app.sys?.recordingsDir}>{app.sys?.recordingsDir}</span>
          <button class="btn sm ghost" onclick={openDir} title="Abrir carpeta"><Icon name="folder" size={14} /></button>
          <button class="btn sm" onclick={pickDir} disabled={rec.active}>Cambiar</button>
        </div>
      </div>
      </div>
      {/if}
    </section>

    <section class="card panel" class:collapsed={!openMeta}>
      <button class="phead" onclick={() => (openMeta = !openMeta)} aria-expanded={openMeta}>
        <h2><Icon name="doc" size={16} /> Detalles de la reunión</h2>
        {#if !openMeta}
          <span class="summary hint">{[app.pendingMeta.date, app.pendingMeta.place, app.pendingMeta.participants.split(/\n|,|;/).map((x) => x.trim()).filter(Boolean).join(", ")].filter(Boolean).join(" · ") || "sin capturar"}</span>
        {/if}
        <span class="chev" class:up={openMeta}><Icon name="chevron" size={14} /></span>
      </button>
      {#if openMeta}
        <div class="pbody">
          <MetaForm bind:meta={app.pendingMeta} hint="Puedes llenarlos mientras grabas; se adjuntan a la grabación y se usan para el resumen y la minuta." />
        </div>
      {/if}
    </section>

    {#if s.liveTranscription}
      <section class="card live" class:full={true}>
        <h2><Icon name="zap" size={16} /> Transcripción en vivo</h2>
        <div class="livebox scroll" bind:this={liveEl}>
          {#if app.liveSegments.length === 0}
            <p class="hint">{rec.active ? "Esperando el primer bloque de audio…" : "Aquí aparecerá el texto conforme se grabe."}</p>
          {:else}
            {#each app.liveSegments as seg, i (i)}
              <div class="seg"><span class="ts">{fmtTimestamp(seg.startMs)}</span><span>{#if seg.speaker}<span class="spk" class:other={seg.speaker === "Interlocutor"}>{seg.speaker}</span> {/if}{seg.text}</span></div>
            {/each}
          {/if}
        </div>
      </section>
    {/if}

  </div>
</div>

<style>
  .top { padding: 22px 26px 14px; }
  .content { flex: 1; min-height: 0; padding: 0 26px 26px; display: flex; flex-direction: column; gap: 14px; }
  .rec { padding: 22px 24px; display: flex; flex-direction: column; gap: 18px; transition: border-color 0.2s; }
  .rec.live { border-color: var(--danger); }
  .rec-main { display: flex; align-items: center; gap: 22px; }
  .rec-btn { width: 84px; height: 84px; border-radius: 50%; display: grid; place-items: center; background: var(--danger); color: #fff; box-shadow: 0 6px 18px color-mix(in srgb, var(--danger) 40%, transparent); transition: transform 0.1s, filter 0.15s; flex-shrink: 0; }
  .rec-btn:hover:not(:disabled) { filter: brightness(1.08); }
  .rec-btn:active:not(:disabled) { transform: scale(0.96); }
  .rec-btn.stop { animation: pulse 1.6s ease-in-out infinite; }
  @keyframes pulse { 0%, 100% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--danger) 45%, transparent); } 50% { box-shadow: 0 0 0 16px transparent; } }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .timer { font-size: 38px; font-weight: 650; letter-spacing: 0.02em; font-variant-numeric: tabular-nums; line-height: 1.1; }
  .timer.idle { color: var(--muted); }
  .err { margin-top: 6px; color: var(--warn); font-size: 13px; display: flex; gap: 6px; align-items: center; }
  .meters { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .meter { display: flex; flex-direction: column; gap: 5px; font-size: 12.5px; color: var(--text-2); }
  .meter span { display: inline-flex; align-items: center; gap: 6px; }
  .meter.off { opacity: 0.4; }
  .bar { height: 8px; border-radius: 999px; background: var(--surface-3); overflow: hidden; }
  .bar > div { height: 100%; background: linear-gradient(90deg, var(--success), var(--warn) 80%, var(--danger)); transition: width 0.12s linear; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; align-items: start; }
  .live.full { grid-column: 1 / -1; }
  .livebox { max-height: 320px; min-height: 90px; user-select: text; }
  .seg { display: grid; grid-template-columns: 58px 1fr; gap: 10px; padding: 4px 0; border-bottom: 1px dashed var(--border); line-height: 1.5; }
  .ts { font-family: var(--mono); font-size: 12px; color: var(--muted); padding-top: 2px; }
  .spk { display: inline-block; padding: 0 6px; margin-right: 2px; border-radius: 6px; font-size: 12px; font-weight: 650; background: var(--accent-soft); color: var(--accent); }
  .spk.other { background: var(--warn-soft); color: var(--warn); }
  .livetag { display: inline-flex; align-items: center; gap: 4px; margin-left: 6px; color: var(--accent); font-weight: 600; }
  section { padding: 18px 20px; display: flex; flex-direction: column; gap: 14px; }
  section h2 { display: flex; align-items: center; gap: 8px; }
  section.panel { padding: 0; gap: 0; }
  .phead { width: 100%; display: flex; align-items: center; gap: 10px; padding: 14px 20px; text-align: left; border-radius: var(--radius); }
  .phead:hover { background: var(--surface-2); }
  .phead h2 { flex-shrink: 0; }
  .summary { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .chev { margin-left: auto; display: inline-flex; color: var(--muted); transition: transform 0.15s; }
  .chev.up { transform: rotate(180deg); }
  .pbody { display: flex; flex-direction: column; gap: 14px; padding: 4px 20px 18px; }
  .switchrow { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .switchrow > div { min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .input.small { margin-top: 2px; }
  .dir { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .path { flex: 1; font-size: 12.5px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  @media (max-width: 1100px) { .grid { grid-template-columns: 1fr; } }
</style>
