<script lang="ts">
  import { app, cancelJob, removeJob, retryJob, isActive, type Job } from "../lib/state.svelte";
  import { fmtBytes, fmtDuration, fmtSpeed } from "../lib/format";
  import Icon from "./Icon.svelte";

  let { job, selected, onselect }: { job: Job; selected: boolean; onselect: () => void } = $props();

  const stageLabel: Record<string, string> = {
    queued: "En cola",
    decoding: "Leyendo audio",
    loading: "Cargando modelo",
    transcribing: "Transcribiendo",
    done: "Listo",
    error: "Error",
    cancelled: "Cancelado",
  };
  let elapsed = $derived(job.startedAt ? ((job.finishedAt ?? app.now) - job.startedAt) / 1000 : 0);
  let video = $derived(/\.(mp4|m4v|mov|mkv|webm|avi|mpe?g|3gp|wmv|ts)$/i.test(job.name));
</script>

<div class="card job" class:selected role="button" tabindex="0" onclick={onselect} onkeydown={(e) => e.key === "Enter" && onselect()}>
  <div class="head">
    <div class="ficon" class:active={isActive(job)} class:done={job.status === "done"} class:err={job.status === "error"}>
      {#if isActive(job)}
        <span class="spin"><Icon name="loader" size={16} /></span>
      {:else if job.status === "done"}
        <Icon name="check" size={16} stroke={2.5} />
      {:else if job.status === "error"}
        <Icon name="alert" size={16} />
      {:else}
        <Icon name={video ? "monitor" : "waveform"} size={16} />
      {/if}
    </div>
    <div class="info">
      <div class="name" title={job.path}>{job.name}</div>
      <div class="meta">{fmtDuration(job.durationSecs)} · {fmtBytes(job.sizeBytes)}</div>
    </div>
    <div class="actions">
      {#if isActive(job)}
        <button class="btn icon ghost" title="Cancelar" onclick={(e) => { e.stopPropagation(); cancelJob(job.id); }}><Icon name="x" size={15} /></button>
      {:else}
        {#if job.status === "error" || job.status === "cancelled"}
          <button class="btn icon ghost" title="Reintentar" onclick={(e) => { e.stopPropagation(); retryJob(job.id); }}><Icon name="refresh" size={15} /></button>
        {/if}
        <button class="btn icon ghost" title="Quitar de la lista" onclick={(e) => { e.stopPropagation(); removeJob(job.id); }}><Icon name="x" size={15} /></button>
      {/if}
    </div>
  </div>

  {#if isActive(job)}
    <div class="progress" class:indeterminate={job.status !== "transcribing"}><div style="width:{job.percent}%"></div></div>
    <div class="status">
      <span>{stageLabel[job.status]}{job.status === "transcribing" ? ` · ${job.percent}%` : ""}</span>
      <span>{fmtDuration(elapsed)}</span>
    </div>
  {:else if job.status === "done" && job.result}
    <div class="status">
      <span class="pill success"><Icon name="check" size={12} stroke={3} /> Listo</span>
      <span title="Tiempo de transcripción">{fmtDuration(job.result.elapsedSecs)} · {fmtSpeed(job.result.audioSecs, job.result.elapsedSecs)} tiempo real</span>
    </div>
  {:else if job.status === "error"}
    <div class="status"><span class="pill danger">Error</span><span class="errtext" title={job.error}>{job.error}</span></div>
  {:else if job.status === "cancelled"}
    <div class="status"><span class="pill warn">Cancelado</span></div>
  {:else}
    <div class="status"><span class="pill">En cola</span></div>
  {/if}
</div>

<style>
  .job { padding: 12px 14px; display: flex; flex-direction: column; gap: 10px; cursor: pointer; border-width: 1px; transition: border-color 0.15s, box-shadow 0.15s; }
  .job:hover { border-color: var(--border-strong); }
  .job.selected { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); }
  .head { display: flex; align-items: center; gap: 10px; }
  .ficon { width: 34px; height: 34px; border-radius: 9px; display: grid; place-items: center; background: var(--surface-2); color: var(--text-2); flex-shrink: 0; }
  .ficon.active { background: var(--accent-soft); color: var(--accent); }
  .ficon.done { background: var(--success-soft); color: var(--success); }
  .ficon.err { background: var(--danger-soft); color: var(--danger); }
  .spin { display: flex; }
  .info { min-width: 0; flex: 1; }
  .name { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .meta { font-size: 12px; color: var(--muted); }
  .actions { display: flex; gap: 2px; }
  .status { display: flex; justify-content: space-between; align-items: center; gap: 10px; font-size: 12.5px; color: var(--muted); }
  .errtext { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; color: var(--danger); }
</style>
