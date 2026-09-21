import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  type AppId,
  type AppStatus,
  type DavMount,
  api,
  type IureAiOptions,
  type IureBlueprint,
  type IureCrmKind,
  type IureDocRef,
  type IureSessionStatus,
  type IureUploadProgress,
  type DeviceList,
  type DocKind,
  type RecordingStatus,
  type DownloadProgress,
  type ModelInfo,
  type ProgressEvent,
  type Segment,
  type SegmentEvent,
  type Settings,
  type SystemInfo,
  type TranscriptResult,
} from "./api";

export type View = "transcribe" | "record" | "models" | "settings";
export type JobStatus = "queued" | "decoding" | "loading" | "transcribing" | "done" | "error" | "cancelled";

export interface DocState {
  status: "idle" | "loading" | "done" | "error";
  content?: string;
  path?: string;
  error?: string;
}

/** Datos de la reunión que el usuario puede capturar en cualquier momento. */
export interface JobMeta {
  participants: string;
  date: string;
  place: string;
  notes: string;
}

/** Fecha de hoy en español, p. ej. «19 de septiembre de 2026». */
export function todayLabel(): string {
  try {
    return new Date().toLocaleDateString("es-MX", { day: "numeric", month: "long", year: "numeric" });
  } catch {
    return new Date().toISOString().slice(0, 10);
  }
}

export function emptyMeta(): JobMeta {
  return { participants: "", date: todayLabel(), place: "", notes: "" };
}

/** ¿El usuario capturó algo más allá de la fecha sugerida? */
export function metaFilledByUser(m: JobMeta): boolean {
  return !!(m.place.trim() || m.participants.trim() || m.notes.trim() || (m.date.trim() && m.date.trim() !== todayLabel()));
}

export function metaToContext(m: JobMeta): string {
  const lines: string[] = [];
  if (m.date.trim()) lines.push(`Fecha: ${m.date.trim()}`);
  if (m.place.trim()) lines.push(`Lugar: ${m.place.trim()}`);
  if (m.participants.trim()) {
    const people = m.participants.split(/\n|,|;/).map((x) => x.trim()).filter(Boolean);
    lines.push(`Participantes: ${people.join(", ")}`);
  }
  if (m.notes.trim()) lines.push(`Notas adicionales: ${m.notes.trim()}`);
  return lines.join("\n");
}

export function metaFilled(m: JobMeta): boolean {
  return !!(m.date.trim() || m.place.trim() || m.participants.trim() || m.notes.trim());
}

/** Resultado de guardar un trabajo en Iurefficient (carpeta WebDAV, proyecto o CRM). */
export interface IureSaved {
  /** Carpeta WebDAV, o etiqueta del destino (p. ej. «Proyecto EXP-001 · Cliente»). */
  folder: string;
  webUrl: string;
  files: string[];
  savedAt: number;
  mode?: "webdav" | "case" | "crm";
  caseId?: string;
  caseTitle?: string;
  documents?: IureDocRef[];
  crm?: { kind: IureCrmKind; id: string; name: string; activityId: string | null };
  timeEntryId?: string | null;
  /** Documentos generados con el motor de Iurefficient (minuta, resumen…). */
  composed?: ComposedDoc[];
}

export interface ComposedDoc {
  taskId: string;
  blueprintId: string;
  blueprintName: string;
  genre: string;
  documentId: string | null;
  link: string | null;
  /** Copia local descargada a la carpeta de salida. */
  localPath: string | null;
  state: string;
  section: string | null;
  current: number;
  total: number;
  error: string | null;
  startedAt: number;
}

export interface Job {
  id: string;
  path: string;
  name: string;
  sizeBytes: number;
  durationSecs: number | null;
  status: JobStatus;
  percent: number;
  error?: string;
  result?: TranscriptResult;
  liveSegments: Segment[];
  summary: DocState;
  minutes: DocState;
  meta: JobMeta;
  iure?: IureSaved;
  iureUpload?: { fileName: string; index: number; totalFiles: number; sent: number; total: number } | null;
  startedAt?: number;
  finishedAt?: number;
}

export interface Toast {
  id: number;
  kind: "info" | "success" | "error";
  text: string;
}

export interface DownloadState {
  downloaded: number;
  total: number | null;
}

export const app = $state({
  ready: false,
  view: "transcribe" as View,
  settings: null as Settings | null,
  sys: null as SystemInfo | null,
  models: [] as ModelInfo[],
  jobs: [] as Job[],
  selectedJobId: null as string | null,
  running: false,
  dragging: false,
  downloads: {} as Record<string, DownloadState>,
  toasts: [] as Toast[],
  now: Date.now(),
  devices: null as DeviceList | null,
  recording: { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false } as RecordingStatus,
  /** Segmentos transcritos en vivo durante la grabación actual. */
  liveSegments: [] as Segment[],
  recordingBusy: false,
  /** Detalles capturados durante la grabación; se adjuntan al trabajo al detenerla. */
  pendingMeta: { participants: "", date: "", place: "", notes: "" } as JobMeta,
  /** Sesión REST con Iurefficient (null = no comprobada todavía). */
  iureSession: null as IureSessionStatus | null,
  /** Versión nueva disponible en GitHub, si se detectó. */
  updateNotice: null as { version: string; url: string } | null,
  /** Apps de Iurefficient en este equipo (null = sin consultar). */
  apps: null as AppStatus[] | null,
  /** Unidades de IureDav configuradas en este equipo. */
  davMounts: [] as DavMount[],
});

export function appStatus(id: AppId): AppStatus | undefined {
  return app.apps?.find((a) => a.id === id);
}

/** Refresca el estado de las apps; con red consulta también la última versión en GitHub. */
export async function refreshApps(withNetwork: boolean): Promise<void> {
  try {
    app.apps = await api.appsStatus(withNetwork);
  } catch (e) {
    console.warn("apps_status:", e);
  }
  try {
    app.davMounts = await api.iuredavMounts();
  } catch {
    app.davMounts = [];
  }
}

/** Abre un archivo con IureEditor; si no está instalada, ofrece descargarla. */
export async function openWithEditor(path: string): Promise<void> {
  const ed = appStatus("editor");
  if (ed && !ed.installed) {
    const { ask } = await import("@tauri-apps/plugin-dialog");
    const go = await ask("IureEditor no está instalada en este equipo. ¿Abrir la página de descarga?", { title: "IureTranscribe", kind: "info", okLabel: "Descargar", cancelLabel: "Cancelar" });
    if (go) openUrl(ed.downloadUrl).catch(() => {});
    return;
  }
  try {
    await api.openWithApp("editor", path);
  } catch (e) {
    toast(String(e), "error", 6000);
  }
}

/**
 * Argumentos de arranque o de una segunda instancia: rutas de archivo (se agregan a
 * la cola) o enlaces `iuretranscribe://` (`transcribe?path=…`, `record`, `settings`).
 */
export async function handleLaunchArgs(args: string[]): Promise<void> {
  const paths: string[] = [];
  for (const raw of args) {
    if (!raw) continue;
    if (raw.startsWith("iuretranscribe:")) {
      let u: URL;
      try {
        u = new URL(raw);
      } catch {
        continue;
      }
      const action = (u.host || u.pathname.replace(/^\/+/, "")).replace(/\/+$/, "").toLowerCase();
      if (action === "record" || action === "grabar") app.view = "record";
      else if (action === "settings" || action === "ajustes") app.view = "settings";
      else if (action === "models" || action === "modelos") app.view = "models";
      else {
        for (const p of u.searchParams.getAll("path")) paths.push(p);
        if (!u.searchParams.has("path")) app.view = "transcribe";
      }
    } else if (/^file:/i.test(raw)) {
      try {
        paths.push(decodeURIComponent(new URL(raw).pathname));
      } catch {
        /* ignorar */
      }
    } else {
      paths.push(raw);
    }
  }
  if (paths.length) {
    app.view = "transcribe";
    const added = await addFiles(paths);
    // Quien abre con IureTranscribe quiere transcribir: arranca la cola si está libre.
    if (added.length && !app.running) void startQueue();
  }
}

let toastSeq = 0;
let stopRequested = false;
let notifyOk: boolean | null = null;

/** Notificación del sistema (además del aviso dentro de la app), si el usuario lo permite. */
export async function notify(title: string, body: string) {
  try {
    if (notifyOk === null) {
      notifyOk = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    }
    if (notifyOk && !document.hasFocus()) sendNotification({ title, body });
  } catch {
    /* sin notificaciones del sistema */
  }
}

export function toast(text: string, kind: Toast["kind"] = "info", ms = 4500) {
  const id = ++toastSeq;
  app.toasts.push({ id, kind, text });
  setTimeout(() => {
    const i = app.toasts.findIndex((t) => t.id === id);
    if (i >= 0) app.toasts.splice(i, 1);
  }, ms);
}

export function applyTheme() {
  const pref = app.settings?.theme ?? "system";
  const dark = pref === "dark" || (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

/** Forma en que se guardan los trabajos en disco (sin segmentos en vivo). */
type StoredJob = Omit<Job, "liveSegments">;

function serializeJobs(): string {
  const stored: StoredJob[] = app.jobs.map((j) => ({
    id: j.id,
    path: j.path,
    name: j.name,
    sizeBytes: j.sizeBytes,
    durationSecs: j.durationSecs,
    status: isActive(j) ? "queued" : j.status,
    percent: isActive(j) ? 0 : j.percent,
    error: j.error,
    result: j.result,
    summary: j.summary.status === "loading" ? { status: "idle" } : j.summary,
    minutes: j.minutes.status === "loading" ? { status: "idle" } : j.minutes,
    meta: { participants: j.meta.participants, date: j.meta.date, place: j.meta.place, notes: j.meta.notes },
    iure: j.iure,
    startedAt: j.startedAt,
    finishedAt: j.finishedAt,
  }));
  return JSON.stringify(stored);
}

let persistTimer: ReturnType<typeof setTimeout> | undefined;
function startPersistence() {
  $effect.root(() => {
    $effect(() => {
      const json = serializeJobs(); // lee sólo los campos persistidos → se re-ejecuta cuando cambian
      clearTimeout(persistTimer);
      persistTimer = setTimeout(() => api.saveJobs(json).catch((e) => console.error("saveJobs", e)), 400);
    });
  });
}

async function restoreJobs() {
  let stored: StoredJob[] = [];
  try {
    stored = JSON.parse(await api.loadJobs());
  } catch (e) {
    console.error("loadJobs", e);
    return;
  }
  if (!Array.isArray(stored) || !stored.length) return;
  const jobs: Job[] = stored.map((j) => ({
    ...j,
    iure: j.iure && j.iure.composed && !Array.isArray(j.iure.composed) ? { ...j.iure, composed: [j.iure.composed as unknown as ComposedDoc] } : j.iure,
    status: isActive(j as Job) ? "queued" : j.status,
    liveSegments: [],
    summary: j.summary ?? { status: "idle" },
    minutes: j.minutes ?? { status: "idle" },
    meta: { ...emptyMeta(), ...(j.meta ?? {}) },
  }));
  // Recupera resumen/minuta que ya existan en disco (p. ej. tras un reinicio).
  await Promise.all(
    jobs
      .filter((j) => j.result && (j.summary.status !== "done" || j.minutes.status !== "done"))
      .map(async (j) => {
        try {
          const docs = await api.loadDocuments(j.result!.outputDir, j.result!.baseName);
          if (docs.summary && j.summary.status !== "done") j.summary = { status: "done", content: docs.summary.content, path: docs.summary.path };
          if (docs.minutes && j.minutes.status !== "done") j.minutes = { status: "done", content: docs.minutes.content, path: docs.minutes.path };
        } catch { /* sin documentos */ }
      }),
  );
  app.jobs = jobs;
  app.selectedJobId = jobs.find((j) => j.status === "done")?.id ?? jobs[0]?.id ?? null;
  const interrupted = stored.filter((j) => isActive(j as Job)).length;
  if (interrupted) toast(`${interrupted} transcripción(es) se interrumpieron al cerrar la app y volvieron a la cola`, "info", 7000);
}

export async function init() {
  const [settings, sys, models] = await Promise.all([api.getSettings(), api.systemInfo(), api.listModels()]);
  app.settings = settings;
  app.sys = sys;
  app.models = models;
  applyTheme();
  await restoreJobs();
  startPersistence();
  refreshIureSession();
  if (settings.checkUpdates) {
    setTimeout(() => {
      import("./updater").then((m) => m.checkForUpdates(true)).catch(() => {});
    }, 6000);
  }
  pollRecording().then(() => {
    if (app.recording.active && !pollTimer) pollTimer = setInterval(pollRecording, 250);
  });
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);

  await listen<ProgressEvent>("job-progress", (e) => {
    const job = app.jobs.find((j) => j.id === e.payload.jobId);
    if (!job || job.status === "done" || job.status === "error" || job.status === "cancelled") return;
    job.status = e.payload.stage;
    job.percent = e.payload.percent;
  });
  await listen<SegmentEvent>("job-segment", (e) => {
    const job = app.jobs.find((j) => j.id === e.payload.jobId);
    if (job && job.status !== "done") job.liveSegments.push(e.payload.segment);
  });
  await listen<DownloadProgress>("model-download-progress", (e) => {
    const p = e.payload;
    if (p.status === "downloading") {
      app.downloads[p.id] = { downloaded: p.downloaded, total: p.total };
    } else {
      delete app.downloads[p.id];
      refreshModels();
      const name = app.models.find((m) => m.id === p.id)?.name ?? p.id;
      if (p.status === "done") toast(`Modelo ${name} descargado`, "success");
      else if (p.status === "error") toast(`Error descargando ${name}: ${p.message ?? ""}`, "error", 8000);
    }
  });
  await listen<IureUploadProgress>("iure-upload-progress", (e) => {
    const job = app.jobs.find((j) => j.id === e.payload.jobId);
    if (job) job.iureUpload = { fileName: e.payload.fileName, index: e.payload.index, totalFiles: e.payload.totalFiles, sent: e.payload.sent, total: e.payload.total };
  });
  await listen<Segment>("live-segment", (e) => {
    app.liveSegments.push(e.payload);
  });
  await getCurrentWebview().onDragDropEvent((event) => {
    const t = event.payload.type;
    if (t === "enter" || t === "over") app.dragging = true;
    else if (t === "leave") app.dragging = false;
    else if (t === "drop") {
      app.dragging = false;
      addFiles(event.payload.paths);
    }
  });
  setInterval(() => (app.now = Date.now()), 1000);
  app.ready = true;
  // Apps hermanas y unidades de IureDav (sin red); archivos o enlaces con los que se abrió.
  refreshApps(false);
  await listen<string[]>("launch-args", (e) => void handleLaunchArgs(e.payload));
  try {
    const { getCurrent, onOpenUrl } = await import("@tauri-apps/plugin-deep-link");
    await onOpenUrl((urls) => void handleLaunchArgs(urls));
    const current = await getCurrent();
    if (current?.length) void handleLaunchArgs(current);
    else void api.launchArgs().then((a) => { if (a.length) void handleLaunchArgs(a); });
  } catch (e) {
    console.warn("deep-link:", e);
    void api.launchArgs().then((a) => { if (a.length) void handleLaunchArgs(a); });
  }
}

export async function refreshModels() {
  app.models = await api.listModels();
}

export async function saveSettings(patch: Partial<Settings>) {
  if (!app.settings) return;
  Object.assign(app.settings, patch);
  applyTheme();
  try {
    await api.saveSettings($state.snapshot(app.settings));
  } catch (e) {
    toast(`No se pudieron guardar los ajustes: ${e}`, "error");
  }
}

export function selectedModel(): ModelInfo | undefined {
  return app.models.find((m) => m.id === app.settings?.modelId);
}

export function downloadedModels(): ModelInfo[] {
  return app.models.filter((m) => m.downloaded);
}

export async function addFiles(paths: string[]): Promise<Job[]> {
  const fresh = paths.filter((p) => !app.jobs.some((j) => j.path === p));
  if (!fresh.length) return [];
  const probes = await api.probeFiles(fresh);
  let skipped = 0;
  const added: Job[] = [];
  for (const p of probes) {
    if (!p.supported) {
      skipped++;
      continue;
    }
    added.push({
      id: crypto.randomUUID(),
      path: p.path,
      name: p.name,
      sizeBytes: p.sizeBytes,
      durationSecs: p.durationSecs,
      status: "queued",
      percent: 0,
      liveSegments: [],
      summary: { status: "idle" },
      minutes: { status: "idle" },
      meta: emptyMeta(),
    });
  }
  app.jobs.push(...added);
  if (!app.selectedJobId && app.jobs.length) app.selectedJobId = app.jobs[0].id;
  if (skipped) toast(`${skipped} archivo(s) omitido(s): formato no compatible`, "error");
  app.view = "transcribe";
  return app.jobs.filter((j) => added.some((a) => a.id === j.id));
}

// ---------------------------------------------------------------------------
// Grabación
// ---------------------------------------------------------------------------
let pollTimer: ReturnType<typeof setInterval> | undefined;

export async function loadDevices() {
  try {
    app.devices = await api.listAudioDevices();
  } catch (e) {
    toast(`No se pudieron listar los dispositivos de audio: ${e}`, "error");
  }
}

async function pollRecording() {
  try {
    app.recording = await api.recordingStatus();
  } catch {
    /* ignorar */
  }
  if (!app.recording.active) stopPolling();
}

function stopPolling() {
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = undefined;
}

/** Nombres para etiquetar [micrófono, sistema] a partir de tu nombre y los asistentes capturados. */
export function speakerNames(): [string, string] {
  const s = app.settings;
  const me = (s?.myName?.trim() || app.iureSession?.name?.trim() || "Yo").split(" ").slice(0, 2).join(" ");
  const others = app.pendingMeta.participants
    .split(/\n|,|;/)
    .map((x) => x.trim())
    .filter((x) => x && !x.toLowerCase().includes(me.toLowerCase().split(" ")[0]));
  const other = others.length === 1 ? others[0].split(" ").slice(0, 2).join(" ") : "Interlocutor";
  return [me, other];
}

/** Texto de un segmento con el hablante como prefijo. */
export function segmentLine(s: Segment): string {
  return s.speaker ? `${s.speaker}: ${s.text.trim()}` : s.text.trim();
}

export async function startRecording() {
  if (app.recordingBusy || app.recording.active) return;
  app.recordingBusy = true;
  app.liveSegments = [];
  try {
    const split = !!app.settings?.speakerSplit && !!app.settings?.recordMic && !!app.settings?.recordSystem;
    const info = await api.startRecording(split ? speakerNames() : null);
    if (info.liveNote) toast(info.liveNote, "info", 7000);
    await pollRecording();
    if (!pollTimer) pollTimer = setInterval(pollRecording, 250);
  } catch (e) {
    toast(String(e), "error", 8000);
  } finally {
    app.recordingBusy = false;
  }
}

export async function stopRecording() {
  if (app.recordingBusy || !app.recording.active) return;
  app.recordingBusy = true;
  stopPolling();
  try {
    const recordingStartedAt = Date.now() - app.recording.elapsedSecs * 1000;
    const wasLive = app.recording.live;
    const res = await api.stopRecording();
    app.recording = { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false };
    const [job] = await addFiles([res.path]);
    const live = app.liveSegments.map((s) => ({ ...s }));
    app.liveSegments = [];
    if (job) {
      job.meta = { ...app.pendingMeta };
      job.liveSegments = live;
      app.selectedJobId = job.id;
    }
    app.pendingMeta = emptyMeta();
    toast(`Grabación guardada (${Math.round(res.durationSecs)} s)`, "success");
    const s = app.settings;
    if (job && wasLive && live.length && s?.liveIsFinal) {
      // La transcripción en vivo es el resultado final: se escriben las salidas sin repetir.
      await finalizeFromLive(job, live, res.durationSecs, (Date.now() - recordingStartedAt) / 1000);
    } else if (s?.autoTranscribeRecording) {
      await startQueue();
    }
  } catch (e) {
    app.recording = { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false };
    toast(String(e), "error", 9000);
  } finally {
    app.recordingBusy = false;
  }
}

export function removeJob(id: string) {
  const i = app.jobs.findIndex((j) => j.id === id);
  if (i < 0) return;
  const job = app.jobs[i];
  if (isActive(job)) return;
  app.jobs.splice(i, 1);
  if (app.selectedJobId === id) app.selectedJobId = app.jobs[0]?.id ?? null;
}

export function clearFinished() {
  app.jobs = app.jobs.filter((j) => isActive(j) || j.status === "queued");
  if (!app.jobs.some((j) => j.id === app.selectedJobId)) app.selectedJobId = app.jobs[0]?.id ?? null;
}

export function retryJob(id: string) {
  const job = app.jobs.find((j) => j.id === id);
  if (!job || isActive(job)) return;
  job.status = "queued";
  job.percent = 0;
  job.error = undefined;
  job.result = undefined;
  job.liveSegments = [];
  job.summary = { status: "idle" };
  job.minutes = { status: "idle" };
}

export function isActive(job: Job): boolean {
  return job.status === "decoding" || job.status === "loading" || job.status === "transcribing";
}

export function queuedCount(): number {
  return app.jobs.filter((j) => j.status === "queued").length;
}

export async function startQueue() {
  if (app.running) return;
  const model = selectedModel();
  if (!model?.downloaded) {
    toast("Primero descarga un modelo en la sección Modelos", "error");
    app.view = "models";
    return;
  }
  app.running = true;
  stopRequested = false;
  let done = 0;
  try {
    while (!stopRequested) {
      const job = app.jobs.find((j) => j.status === "queued");
      if (!job) break;
      await runJob(job);
      done++;
    }
  } finally {
    app.running = false;
  }
  if (done > 0 && !stopRequested) notify("IureTranscribe", done === 1 ? "Transcripción terminada" : `${done} transcripciones terminadas`);
}

export function stopQueue() {
  stopRequested = true;
  for (const job of app.jobs) if (isActive(job)) api.cancelJob(job.id);
}

export async function cancelJob(id: string) {
  const job = app.jobs.find((j) => j.id === id);
  if (job && isActive(job)) await api.cancelJob(id);
}

async function runJob(job: Job) {
  job.status = "decoding";
  job.percent = 0;
  job.liveSegments = [];
  job.error = undefined;
  job.startedAt = Date.now();
  if (!app.selectedJobId || !app.jobs.some((j) => j.id === app.selectedJobId && isActive(j))) app.selectedJobId = job.id;
  try {
    const result = await api.transcribeFile(job.id, job.path);
    job.result = result;
    job.durationSecs = result.audioSecs;
    job.status = "done";
    job.percent = 100;
    job.finishedAt = Date.now();
    const s = app.settings;
    if (s?.autoSummary) await generateDoc(job, "summary");
    if (s?.autoMinutes) await generateDoc(job, "minutes");
  } catch (e) {
    const msg = String(e);
    job.finishedAt = Date.now();
    job.status = /cancelad/i.test(msg) ? "cancelled" : "error";
    job.error = msg;
    if (job.status === "error") toast(`Error en ${job.name}: ${msg}`, "error", 8000);
  }
}

async function finalizeFromLive(job: Job, segments: Segment[], audioSecs: number, elapsedSecs: number) {
  job.status = "decoding";
  try {
    const result = await api.saveLiveTranscript(job.id, job.path, segments, audioSecs, elapsedSecs);
    job.result = result;
    job.durationSecs = result.audioSecs;
    job.status = "done";
    job.percent = 100;
    job.startedAt = Date.now() - elapsedSecs * 1000;
    job.finishedAt = Date.now();
    const s = app.settings;
    if (s?.autoSummary) await generateDoc(job, "summary");
    if (s?.autoMinutes) await generateDoc(job, "minutes");
  } catch (e) {
    job.status = "queued";
    toast(`No se pudo guardar la transcripción en vivo: ${e}. Se transcribirá de nuevo.`, "error", 8000);
    await startQueue();
  }
}

/** Comprueba (sin bloquear) si hay sesión REST válida con la instancia. */
export async function refreshIureSession() {
  if (!iureConfigured()) {
    app.iureSession = { loggedIn: false, name: null, email: null, crm: false, terminology: null, error: null };
    return;
  }
  try {
    app.iureSession = await api.iureSessionStatus();
  } catch (e) {
    app.iureSession = { loggedIn: false, name: null, email: null, crm: false, terminology: null, error: String(e) };
  }
}

export function iureLoggedIn(): boolean {
  return !!app.iureSession?.loggedIn;
}

/** Etiquetas de la instancia («proyecto»/«caso», «cliente»/«paciente»), capitalizadas. */
export function term(key: "case" | "cases" | "client" | "clients"): string {
  const t = app.iureSession?.terminology;
  const v = t?.[key] || { case: "proyecto", cases: "proyectos", client: "cliente", clients: "clientes" }[key];
  return v.charAt(0).toUpperCase() + v.slice(1);
}

/** Horas de la reunión redondeadas a cuartos, a partir de la duración del audio. */
export function suggestedHours(job: Job): number {
  const secs = job.result?.audioSecs ?? job.durationSecs ?? 0;
  return Math.max(0.25, Math.round((secs / 3600) * 4) / 4);
}

export async function saveToCase(job: Job, caseId: string, caseTitle: string, includeMedia: boolean, hours: number | null): Promise<boolean> {
  const files = iureFilesFor(job, includeMedia);
  if (!files.length) {
    toast("No hay archivos que subir todavía", "error");
    return false;
  }
  job.iureUpload = { fileName: "", index: 0, totalFiles: files.length, sent: 0, total: 0 };
  try {
    const transcriptPath = transcriptPathOf(job);
    const res = await api.iureUploadToCase({ jobId: job.id, caseId, files, transcriptPath, hours, hoursDescription: hours ? `Reunión: ${job.name}` : null });
    job.iure = { mode: "case", folder: caseTitle, webUrl: res.webUrl, files: res.documents.map((d) => d.fileName), savedAt: Date.now(), caseId, caseTitle, documents: res.documents, timeEntryId: res.timeEntryId, composed: [] };
    toast(`Guardado en ${caseTitle} (${res.documents.length} documento(s)${res.timeEntryId ? ", horas registradas" : ""})`, "success", 6000);
    autoComposeMinutes(job); // en segundo plano
    return true;
  } catch (e) {
    toast(`No se pudo guardar en Iurefficient: ${e}`, "error", 9000);
    return false;
  } finally {
    job.iureUpload = null;
  }
}

export async function saveToCrm(job: Job, kind: IureCrmKind, id: string, name: string, includeMedia: boolean, withActivity: boolean): Promise<boolean> {
  const files = iureFilesFor(job, includeMedia);
  if (!files.length) {
    toast("No hay archivos que subir todavía", "error");
    return false;
  }
  job.iureUpload = { fileName: "", index: 0, totalFiles: files.length, sent: 0, total: 0 };
  try {
    const minutes = Math.max(1, Math.round((job.result?.audioSecs ?? job.durationSecs ?? 0) / 60));
    const description = job.summary.status === "done" && job.summary.content ? job.summary.content : job.result?.text.slice(0, 2000) ?? null;
    const res = await api.iureUploadToCrm({
      jobId: job.id,
      kind,
      id,
      files,
      activitySubject: withActivity ? `Reunión: ${job.name.replace(/\.[^.]+$/, "")}` : null,
      activityDescription: withActivity ? description : null,
      durationMinutes: withActivity ? minutes : null,
    });
    const label = `${kind === "lead" ? "Lead" : "Oportunidad"} · ${name}`;
    job.iure = { mode: "crm", folder: label, webUrl: res.webUrl, files: res.documents.map((d) => d.fileName), savedAt: Date.now(), documents: res.documents, crm: { kind, id, name, activityId: res.activityId } };
    toast(`Adjuntado a ${label}${res.activityId ? " y actividad registrada" : ""}`, "success", 6000);
    return true;
  } catch (e) {
    toast(`No se pudo guardar en el CRM: ${e}`, "error", 9000);
    return false;
  } finally {
    job.iureUpload = null;
  }
}

/** Ruta local de la transcripción en texto (txt, o srt como respaldo). */
export function transcriptPathOf(job: Job): string | null {
  return job.result?.outputs.find((o) => o.format === "txt")?.path ?? job.result?.outputs.find((o) => o.format === "srt")?.path ?? null;
}

/** Sube únicamente la transcripción a la instancia (a un proyecto o sin proyecto) para poder componer con el motor. */
export async function uploadTranscriptOnly(job: Job, caseId: string | null, caseTitle: string | null): Promise<boolean> {
  const path = transcriptPathOf(job);
  if (!path) {
    toast("No hay transcripción que subir todavía", "error");
    return false;
  }
  job.iureUpload = { fileName: "", index: 0, totalFiles: 1, sent: 0, total: 0 };
  try {
    const res = await api.iureUploadToCase({ jobId: job.id, caseId, files: [path], transcriptPath: path, hours: null, hoursDescription: null });
    const label = caseTitle ?? "General (sin proyecto)";
    const prev = job.iure;
    job.iure = {
      mode: "case",
      folder: prev?.mode === "webdav" ? prev.folder : label,
      webUrl: res.webUrl,
      files: [...(prev?.files ?? []), ...res.documents.map((d) => d.fileName)],
      savedAt: Date.now(),
      caseId: caseId ?? undefined,
      caseTitle: caseTitle ?? undefined,
      documents: [...(prev?.documents ?? []).filter((d) => !d.isTranscript), ...res.documents],
      composed: prev?.composed ?? [],
      crm: prev?.crm,
      timeEntryId: prev?.timeEntryId,
    };
    return true;
  } catch (e) {
    toast(`No se pudo subir la transcripción: ${e}`, "error", 9000);
    return false;
  } finally {
    job.iureUpload = null;
  }
}

/** Documento de transcripción subido a la instancia para este trabajo. */
export function iureTranscriptDoc(job: Job): IureDocRef | undefined {
  return job.iure?.documents?.find((d) => d.isTranscript) ?? job.iure?.documents?.[0];
}

/** Espera a que la instancia tenga texto del documento (extracción asíncrona) y devuelve las opciones. */
export async function iureWaitAiOptions(docId: string, tries = 10): Promise<IureAiOptions> {
  let last: IureAiOptions | null = null;
  for (let i = 0; i < tries; i++) {
    last = await api.iureAiOptions(docId);
    if (last.canGenerate || last.reason !== "no_text") return last;
    await new Promise((r) => setTimeout(r, 3000));
  }
  return last!;
}

/** Genera un documento (minuta, resumen…) con el motor de Iurefficient a partir de la transcripción subida. */
export async function composeWithIurefficient(job: Job, blueprint: { id: string; name: string; genre: string }): Promise<boolean> {
  const transcript = iureTranscriptDoc(job);
  if (!job.iure || !transcript) {
    toast("Primero guarda la transcripción en un proyecto de Iurefficient", "error");
    return false;
  }
  const attendees = job.meta.participants.split(/\n|,|;/).map((x) => x.trim()).filter(Boolean);
  try {
    const taskId = await api.iureCompose({
      blueprintId: blueprint.id,
      caseId: job.iure.caseId ?? null,
      sourceDocumentIds: [transcript.id],
      title: `${blueprint.name} · ${job.name.replace(/\.[^.]+$/, "")}`,
      attendees,
      extraInstructions: metaToContext(job.meta) || null,
    });
    if (!job.iure.composed) job.iure.composed = [];
    job.iure.composed.push({ taskId, blueprintId: blueprint.id, blueprintName: blueprint.name, genre: blueprint.genre, documentId: null, link: null, localPath: null, state: "PENDING", section: null, current: 0, total: 0, error: null, startedAt: Date.now() });
    toast(`Generando «${blueprint.name}» en Iurefficient…`, "info", 4000);
    pollCompose(job);
    return true;
  } catch (e) {
    toast(`No se pudo iniciar la generación: ${e}`, "error", 9000);
    return false;
  }
}

/** Resumen con la IA de la instancia usando el prompt de resumen de la app (sin llave local). */
export async function summaryWithIurefficient(job: Job): Promise<boolean> {
  const transcript = iureTranscriptDoc(job);
  if (!transcript) {
    toast("Primero sube la transcripción a Iurefficient", "error");
    return false;
  }
  if (job.summary.status === "loading") return false;
  job.summary = { status: "loading" };
  try {
    const res = await api.iureSummaryViaChat(transcript.id, job.result!.outputDir, job.result!.baseName);
    job.summary = { status: "done", content: res.content, path: res.path };
    toast("Resumen generado con la IA de Iurefficient", "success", 6000);
    notify("IureTranscribe", `Resumen generado con Iurefficient para ${job.name}`);
    return true;
  } catch (e) {
    job.summary = { status: "error", error: String(e) };
    toast(`No se pudo generar el resumen en Iurefficient: ${e}`, "error", 9000);
    return false;
  }
}

/** Tras guardar en un proyecto: lanza el formato de minuta sugerido por la instancia. */
export async function autoComposeMinutes(job: Job) {
  const transcript = iureTranscriptDoc(job);
  if (!transcript || !app.settings?.iureAutoCompose) return;
  try {
    const o = await iureWaitAiOptions(transcript.id);
    if (!o.canGenerate) {
      toast(o.reason === "no_text" ? "Iurefficient aún no ha extraído el texto; genera la minuta desde la pestaña Minuta en un momento." : "Iurefficient no puede generar a partir de ese documento.", "info", 8000);
      return;
    }
    const isMinuta = (b: IureBlueprint) => /minuta|minute|acta/i.test(`${b.genre} ${b.name}`);
    const pick = o.blueprints.find((b) => b.suggested && isMinuta(b)) ?? o.blueprints.find(isMinuta) ?? o.blueprints.find((b) => b.suggested);
    if (!pick) {
      toast("La instancia no tiene un formato de minuta publicado; elige uno en la pestaña Minuta.", "info", 8000);
      return;
    }
    await composeWithIurefficient(job, pick);
  } catch (e) {
    toast(`No se pudo generar la minuta automáticamente: ${e}`, "error", 8000);
  }
}

const composePolls = new Map<string, ReturnType<typeof setInterval>>();

export function pollCompose(job: Job) {
  if (composePolls.has(job.id)) return;
  const tick = async () => {
    const pending = (job.iure?.composed ?? []).filter((c) => !c.error && c.state !== "SUCCESS" && c.state !== "FAILURE");
    if (!pending.length || !job.iure) return stopComposePoll(job.id);
    for (const c of pending) {
      try {
        const st = await api.iureComposeStatus(c.taskId);
        c.state = st.state;
        c.section = st.section;
        c.current = st.current;
        c.total = st.total;
        if (st.state === "SUCCESS") {
          c.documentId = st.documentId;
          c.link = st.link ? (st.link.startsWith("http") ? st.link : job.iure.webUrl.replace(/\/$/, "") + st.link) : null;
          toast(`«${c.blueprintName}» generada en Iurefficient`, "success", 7000);
          notify("IureTranscribe", `«${c.blueprintName}» generada en Iurefficient para ${job.name}`);
          downloadComposed(job, c);
        } else if (st.state === "FAILURE" || st.state === "REVOKED") {
          c.error = st.error ?? "La generación falló";
          toast(`Iurefficient no pudo generar «${c.blueprintName}»: ${c.error}`, "error", 9000);
          notify("IureTranscribe", `Iurefficient no pudo generar «${c.blueprintName}»`);
        } else if (Date.now() - c.startedAt > 30 * 60 * 1000) {
          c.error = "Sin respuesta de la instancia tras 30 minutos";
        }
      } catch (e) {
        c.error = String(e);
      }
    }
  };
  tick();
  composePolls.set(job.id, setInterval(tick, 3000));
}

function stopComposePoll(jobId: string) {
  const t = composePolls.get(jobId);
  if (t) clearInterval(t);
  composePolls.delete(jobId);
}

/** Descarga el documento generado a la carpeta de salida del trabajo (con su extensión real)
 *  y, si es texto o Markdown, lo muestra como contenido de la pestaña correspondiente. */
export async function downloadComposed(job: Job, c: ComposedDoc) {
  if (!c.documentId || !job.result) return;
  const base = job.result.baseName;
  const slug = c.blueprintName.toLowerCase().replace(/[^a-z0-9áéíóúñü]+/gi, "-").replace(/^-|-$/g, "");
  try {
    c.localPath = await api.iureDownloadDocument(c.documentId, job.result.outputDir, `${base}_${slug}`);
    if (/\.(md|markdown|txt)$/i.test(c.localPath)) {
      const content = await api.readTextFile(c.localPath);
      const slot = /resumen|summary|síntesis|sintesis/i.test(`${c.genre} ${c.blueprintName}`) ? job.summary : job.minutes;
      slot.status = "done";
      slot.content = content;
      slot.path = c.localPath;
      slot.error = undefined;
    }
  } catch (e) {
    console.warn("descarga de documento generado", e);
  }
}

export function iureConfigured(): boolean {
  const s = app.settings;
  return !!(s && s.iureDomain.trim() && s.iureEmail.trim() && s.iureAppPassword.trim());
}

/** Archivos locales que se subirían a Iurefficient para un trabajo terminado. */
export function iureFilesFor(job: Job, includeMedia: boolean): string[] {
  const files: string[] = [];
  if (includeMedia) files.push(job.path);
  if (job.result) for (const o of job.result.outputs) files.push(o.path);
  if (job.summary.status === "done" && job.summary.path) files.push(job.summary.path);
  if (job.minutes.status === "done" && job.minutes.path) files.push(job.minutes.path);
  return files;
}

export async function saveToIurefficient(job: Job, folder: string, includeMedia: boolean): Promise<boolean> {
  const files = iureFilesFor(job, includeMedia);
  if (!files.length) {
    toast("No hay archivos que subir todavía", "error");
    return false;
  }
  job.iureUpload = { fileName: "", index: 0, totalFiles: files.length, sent: 0, total: 0 };
  try {
    const res = await api.iureUpload(job.id, folder, files);
    job.iure = { folder: res.folder, webUrl: res.webUrl, files: res.uploaded.map((u) => u.fileName), savedAt: Date.now() };
    if (app.settings) app.settings.iureLastFolder = res.folder;
    const versions = res.uploaded.filter((u) => !u.created).length;
    const renamed = res.uploaded.filter((u) => u.renamedFrom);
    toast(`Guardado en Iurefficient (${res.uploaded.length} archivo(s)${versions ? `, ${versions} como versión nueva` : ""})`, "success", 6000);
    if (renamed.length) toast(`La instancia no admite ${renamed.map((u) => u.renamedFrom!.split(".").pop()).join("/")}: se guardó como ${renamed.map((u) => u.fileName).join(", ")}`, "info", 9000);
    return true;
  } catch (e) {
    toast(`No se pudo guardar en Iurefficient: ${e}`, "error", 9000);
    return false;
  } finally {
    job.iureUpload = null;
  }
}

/** Vuelve a transcribir un archivo ya terminado con el proceso completo (calidad alta). */
export async function retranscribe(id: string) {
  retryJob(id);
  await startQueue();
}

export async function generateDoc(job: Job, kind: DocKind) {
  if (!job.result) return;
  const slot = kind === "summary" ? job.summary : job.minutes;
  if (slot.status === "loading") return;
  slot.status = "loading";
  slot.error = undefined;
  try {
    const res = await api.generateDocument(kind, job.result.text, job.result.outputDir, job.result.baseName, metaToContext(job.meta));
    slot.status = "done";
    slot.content = res.content;
    slot.path = res.path;
  } catch (e) {
    slot.status = "error";
    slot.error = String(e);
  }
}

export async function downloadModel(id: string) {
  app.downloads[id] = { downloaded: 0, total: null };
  await refreshModels();
  try {
    await api.downloadModel(id);
  } catch (e) {
    delete app.downloads[id];
    await refreshModels();
    if (!/cancelad/i.test(String(e))) toast(String(e), "error", 8000);
  }
}

export async function cancelDownload(id: string) {
  await api.cancelDownload(id);
}

export async function deleteModel(id: string) {
  try {
    await api.deleteModel(id);
    await refreshModels();
    toast("Modelo eliminado", "info");
  } catch (e) {
    toast(String(e), "error");
  }
}
