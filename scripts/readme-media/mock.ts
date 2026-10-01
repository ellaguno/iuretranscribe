// Simulated backend to capture the real IureTranscribe UI in a browser (README media).
// Only fictional data. Query params: ?lang=en|es  &theme=light|dark
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";

mockWindows("main");

const q = new URLSearchParams(location.search);
const LANG: "en" | "es" = q.get("lang") === "es" ? "es" : "en";
const THEME = q.get("theme") === "dark" ? "dark" : "light";
const L = <T,>(en: T, es: T): T => (LANG === "es" ? es : en);

const HOME = "/home/demo";
const MEET = L(`${HOME}/Documents/Meetings`, `${HOME}/Documentos/Reuniones`);
const RECDIR = L(`${HOME}/Music/IureTranscribe`, `${HOME}/Música/IureTranscribe`);

const settings: any = {
  modelId: "large-v3-turbo", language: LANG, translate: false, formats: ["srt", "txt"],
  outputMode: "same", outputDir: null, useGpu: true, threads: 0, beamSize: 5,
  vocabulary: L(
    "Iurefficient, Bayside Logistics, Ruiz & Associates, estoppel, subrogation, force majeure",
    "Iurefficient, Logística Bahía, Ruiz & Asociados, litisconsorcio, subrogación, CFDI",
  ),
  corrections: L(
    [
      { wrong: "your efficient, Iure efficient", right: "Iurefficient" },
      { wrong: "a stopple, estople", right: "estoppel" },
      { wrong: "Bay side logistics", right: "Bayside Logistics" },
    ],
    [
      { wrong: "Iur eficient, Iure eficiente", right: "Iurefficient" },
      { wrong: "litis consorcio, lítis consorcio", right: "litisconsorcio" },
      { wrong: "Ruiz y asociados", right: "Ruiz & Asociados" },
    ],
  ),
  autoSummary: true, autoMinutes: true, openrouterApiKey: "", openrouterModel: "google/gemini-2.5-flash",
  summaryPrompt: "", minutesPrompt: "", theme: THEME, recordingsDir: null,
  recordMic: true, recordSystem: true, micDevice: null, autoTranscribeRecording: true,
  liveTranscription: true, liveChunkSecs: 8, liveIsFinal: true,
  iureDomain: "demo.iurefficient.com", iureEmail: "ana@example.com", iureAppPassword: "", iureUploadMedia: true, iureLastFolder: null,
  iureAutoCompose: true, speakerSplit: true, myName: "Ana Torres", checkUpdates: false,
  uiLanguage: LANG,
};

const models = [
  { id: "large-v3-turbo", name: "Large v3 Turbo", fileName: "ggml-large-v3-turbo.bin", sizeMb: 1624, description: "", quality: "alta", recommended: true, downloaded: true, downloading: false, bundled: false, path: "/m/ggml-large-v3-turbo.bin" },
  { id: "base", name: "Base", fileName: "ggml-base.bin", sizeMb: 148, description: "", quality: "baja", recommended: false, downloaded: true, downloading: false, bundled: true, path: "/m/ggml-base.bin" },
];

// ---------------------------------------------------------------- stored jobs
const day = (d: string) => new Date(d).getTime();
const seg = (s: number, e: number, text: string, speaker?: string) => ({ startMs: s * 1000, endMs: e * 1000, text, speaker: speaker ?? null });
const ME = "Ana Torres";
const OTHER = L("Other party", "Interlocutor");

const meetingSegs = L(
  [
    seg(0, 6, "Thanks for joining. Today we close the warehouse lease with Bayside Logistics.", ME),
    seg(6, 13, "Our main concern is the early termination penalty in clause nine.", OTHER),
    seg(13, 19, "We can propose lowering it from six to three months of rent.", ME),
    seg(19, 25, "That works if the notice period is sixty days.", OTHER),
    seg(25, 31, "Agreed. I'll send the revised draft on Monday.", ME),
    seg(31, 36, "And the board can sign it on Tuesday the 29th.", OTHER),
  ],
  [
    seg(0, 6, "Gracias por conectarse. Hoy cerramos el arrendamiento de la bodega con Logística Bahía.", ME),
    seg(6, 13, "Lo que más nos preocupa es la penalización por terminación anticipada de la cláusula nueve.", OTHER),
    seg(13, 19, "Podemos proponer bajarla de seis a tres meses de renta.", ME),
    seg(19, 25, "Nos funciona si el aviso es con sesenta días de anticipación.", OTHER),
    seg(25, 31, "De acuerdo. Les envío la versión corregida el lunes.", ME),
    seg(31, 36, "Y el consejo la firma el martes 29.", OTHER),
  ],
);

const SUMMARY = L(
  `## Summary

Ana Torres met with Bayside Logistics to close the **warehouse lease**. Both sides agreed to lower the early termination penalty (clause nine) from six to **three months of rent**, provided notice is given **60 days** in advance.

Ana will send the revised draft on Monday the 28th, and the client's board will sign it on Tuesday the 29th.`,
  `## Resumen

Ana Torres se reunió con Logística Bahía para cerrar el **contrato de arrendamiento de la bodega**. Ambas partes acordaron reducir la penalización por terminación anticipada (cláusula nueve) de seis a **tres meses de renta**, siempre que el aviso se dé con **60 días** de anticipación.

Ana enviará la versión corregida el lunes 28 y el consejo del cliente la firmará el martes 29.`,
);

const MINUTES = L(
  `# Minutes: warehouse lease review

**Date:** September 25, 2026 · **Place:** Video call · **Duration:** 45 min

## Attendees
- Ana Torres (Ruiz & Associates)
- Carlos Ruiz, Laura Méndez (Bayside Logistics)

## Agenda
1. Early termination penalty (clause nine)
2. Notice period
3. Signing schedule

## Decisions
- Early termination penalty: **3 months of rent** (previously 6).
- Notice period for termination: **60 days**.

## Action items
| Owner | Task | Due |
| --- | --- | --- |
| Ana Torres | Send the revised draft of the lease | Mon, Sep 28 |
| Carlos Ruiz | Present it to the board for signature | Tue, Sep 29 |

## Open items
- Confirm whether the deposit is adjusted to the new rent.`,
  `# Minuta: revisión del contrato de arrendamiento

**Fecha:** 25 de septiembre de 2026 · **Lugar:** Videollamada · **Duración:** 45 min

## Asistentes
- Ana Torres (Ruiz & Asociados)
- Carlos Ruiz, Laura Méndez (Logística Bahía)

## Orden del día
1. Penalización por terminación anticipada (cláusula nueve)
2. Plazo de aviso
3. Calendario de firma

## Acuerdos
- Penalización por terminación anticipada: **3 meses de renta** (antes 6).
- Aviso de terminación: **60 días** de anticipación.

## Compromisos
| Responsable | Tarea | Fecha |
| --- | --- | --- |
| Ana Torres | Enviar la versión corregida del contrato | lun 28 sep |
| Carlos Ruiz | Presentarlo al consejo para firma | mar 29 sep |

## Pendientes
- Confirmar si el depósito en garantía se ajusta a la nueva renta.`,
);

const hearingName = L("Preliminary hearing - case 2041.mp3", "Audiencia preliminar - expediente 2041.mp3");
const hearingBase = hearingName.replace(/\.mp3$/, "");
const meetingName = L("Lease review - Bayside Logistics.wav", "Revisión de contrato - Logística Bahía.wav");
const meetingBase = meetingName.replace(/\.wav$/, "");

const storedJobs = [
  {
    id: "job-hearing", path: `${MEET}/${hearingName}`, name: hearingName, sizeBytes: 46_200_000, durationSecs: 2890,
    status: "done", percent: 100,
    result: {
      jobId: "job-hearing", text: "", audioSecs: 2890, elapsedSecs: 251, detectedLanguage: LANG, outputDir: MEET, baseName: hearingBase,
      outputs: [{ format: "srt", path: `${MEET}/${hearingBase}.srt` }, { format: "txt", path: `${MEET}/${hearingBase}.txt` }],
      segments: [seg(0, 8, L("The hearing is now in session for case 2041.", "Se abre la audiencia del expediente 2041."))],
    },
    summary: { status: "idle" }, minutes: { status: "idle" },
    meta: { participants: "", date: L("September 22, 2026", "22 de septiembre de 2026"), place: "", notes: "" },
    startedAt: day("2026-09-22T10:00:00"), finishedAt: day("2026-09-22T10:04:11"),
  },
  {
    id: "job-meeting", path: `${RECDIR}/${meetingName}`, name: meetingName, sizeBytes: 86_400_000, durationSecs: 2712,
    status: "done", percent: 100,
    result: {
      jobId: "job-meeting", text: "", audioSecs: 2712, elapsedSecs: 2712, detectedLanguage: LANG, outputDir: RECDIR, baseName: meetingBase,
      outputs: [{ format: "srt", path: `${RECDIR}/${meetingBase}.srt` }, { format: "txt", path: `${RECDIR}/${meetingBase}.txt` }],
      segments: meetingSegs,
    },
    summary: { status: "done", content: SUMMARY, path: `${RECDIR}/${meetingBase}_summary.md` },
    minutes: { status: "done", content: MINUTES, path: `${RECDIR}/${meetingBase}_minutes.md` },
    meta: {
      participants: "Ana Torres\nCarlos Ruiz\nLaura Méndez",
      date: L("September 25, 2026", "25 de septiembre de 2026"),
      place: L("Video call", "Videollamada"), notes: "",
    },
    startedAt: day("2026-09-25T10:00:00"), finishedAt: day("2026-09-25T10:45:12"),
  },
];
if (q.get("jobs") === "none") storedJobs.length = 0;

// ---------------------------------------------------------------- recording state
const REC = `${RECDIR}/${L("Recording", "Grabación")} 2026-10-01 10-00.wav`;
const rec: any = { active: false, elapsedSecs: 0, micLevel: 0, sysLevel: 0, path: null, error: null, livePendingSecs: 0, live: false };
let speakers: [string, string] | null = null;
const docs: Record<string, (v: any) => void> = {};
let pending: { jobId: string; resolve: (v: any) => void } | null = null;
const segs: any[] = [];

const apps = [
  { id: "transcribe", name: "IureTranscribe", description: "", installed: true, path: "/usr/bin/iuretranscribe", downloadUrl: "https://github.com/ellaguno/iuretranscribe/releases/latest", latestVersion: "0.7.0" },
  { id: "editor", name: "IureEditor", description: "", installed: true, path: "/usr/bin/iureditor", downloadUrl: "https://github.com/ellaguno/iureditor/releases/latest", latestVersion: null },
  { id: "dav", name: "IureDav", description: "", installed: false, path: null, downloadUrl: "https://github.com/ellaguno/iuredav/releases/latest", latestVersion: null },
  { id: "ocr", name: "IureOCR", description: "", installed: false, path: null, downloadUrl: "https://github.com/ellaguno/iureocr/releases/latest", latestVersion: null },
];

const copilotStatus = {
  signedIn: true,
  endpointUrl: "https://demo.iurefficient.com/api/mcp",
  manageUrl: "https://demo.iurefficient.com/settings/mcp",
  tokens: [{ id: "tok-1", name: "Microsoft 365 Copilot", tokenPrefix: "iurmcp_7c1e", isValid: true, createdAt: "2026-09-28T16:20:00Z", expiresAt: "2027-09-28T16:20:00Z", lastUsedAt: "2026-09-30T18:02:00Z", callCount: 37 }],
  error: null,
};

mockIPC(
  (cmd, args: any) => {
    switch (cmd) {
      case "ui_language": return LANG;
      case "default_prompts": return { summary: "", minutes: "" };
      case "get_settings": return structuredClone(settings);
      case "save_settings": Object.assign(settings, args.settings); return null;
      case "system_info": return { backend: "Vulkan", cpuThreads: 16, platform: "linux", modelsDir: `${HOME}/.local/share/com.iurefficient.iuretranscribe/models`, settingsPath: `${HOME}/.config/com.iurefficient.iuretranscribe/settings.json`, logPath: `${HOME}/.local/share/com.iurefficient.iuretranscribe/logs/iuretranscribe.log`, ffmpegAvailable: true, recordingsDir: RECDIR, version: "0.7.0", updateTarget: "linux", supportedExtensions: ["wav", "mp3", "m4a", "mp4", "mov", "mkv", "flac", "ogg"] };
      case "list_models": return models;
      case "load_jobs": return JSON.stringify(storedJobs);
      case "save_jobs": return null;
      case "load_documents": return { summary: null, minutes: null };
      case "iure_session_status": return { loggedIn: true, name: "Ana Torres", email: "ana@example.com", crm: true, terminology: null, error: null };
      case "iure_ai_options": return { canGenerate: true, reason: null, isTranscript: true, composing: false, blueprints: [] };
      case "recording_status": return { ...rec };
      case "start_recording": speakers = args.speakers; rec.active = true; rec.live = true; rec.path = REC; return { path: REC, live: true, liveNote: null };
      case "stop_recording": rec.active = false; return { path: REC, durationSecs: rec.elapsedSecs };
      case "save_live_transcript": {
        const r = args.request;
        const base = REC.split("/").pop()!.replace(/\.wav$/, "");
        return { jobId: r.jobId, segments: r.segments, text: r.segments.map((x: any) => `${x.speaker}: ${x.text}`).join("\n"), audioSecs: r.audioSecs, elapsedSecs: r.audioSecs, outputs: [{ format: "srt", path: `${RECDIR}/${base}.srt` }, { format: "txt", path: `${RECDIR}/${base}.txt` }], outputDir: RECDIR, baseName: base, detectedLanguage: LANG };
      }
      case "generate_document": return new Promise((resolve) => { docs[args.request.kind] = resolve; });
      case "apps_status": return apps;
      case "iuredav_mounts": return [];
      case "launch_args": return [];
      case "agents_status": return {
        claudeDesktop: { installed: true, connected: true, stale: false, configPath: `${HOME}/.config/Claude/claude_desktop_config.json`, downloadUrl: "https://claude.ai/download" },
        vscode: { installed: true, connected: false, stale: false, configPath: null, downloadUrl: "https://code.visualstudio.com/download", copilot: true },
      };
      case "copilot_status": return structuredClone(copilotStatus);
      case "check_update_notice": return null;
      case "list_audio_devices": return { inputs: [], systemCapture: "native", note: L("Captures the audio of the video call or the browser.", "Captura el audio de la videollamada o del navegador."), backend: "PipeWire" };
      case "probe_files": return args.paths.map((p: string, i: number) =>
        p === REC
          ? { path: p, name: p.split("/").pop(), sizeBytes: rec.elapsedSecs * 32_000, durationSecs: rec.elapsedSecs, supported: true }
          : { path: p, name: p.split("/").pop(), sizeBytes: [184_000_000, 22_600_000][i % 2], durationSecs: [3125, 1410][i % 2], supported: true });
      case "transcribe_file":
        return new Promise((resolve) => { pending = { jobId: args.request.jobId, resolve }; });
      case "cancel_job": return true;
      case "plugin:deep-link|get_current": return null;
      case "plugin:notification|is_permission_granted": return true;
      default:
        if (cmd.startsWith("plugin:")) return null;
        console.log("mock: sin respuesta para", cmd);
        return null;
    }
  },
  { shouldMockEvents: true },
);

// ---------------------------------------------------------------- hooks for capture.mjs
const w = window as any;
w.__lang = LANG;
w.__emit = emit;
w.__speakers = () => speakers;
w.__progress = (percent: number) => pending && emit("job-progress", { jobId: pending.jobId, stage: "transcribing", percent });
w.__seg = (startMs: number, endMs: number, text: string) => {
  const s = { startMs, endMs, text };
  segs.push(s);
  return pending && emit("job-segment", { jobId: pending.jobId, segment: s });
};
w.__rec = rec;
w.__live = (startMs: number, endMs: number, text: string, speaker: string) => emit("live-segment", { startMs, endMs, text, speaker });
w.__doc = (kind: string, content: string) => docs[kind]?.({ kind, content, path: `${RECDIR}/${kind}.md` });

await import("../../src/main.ts");
