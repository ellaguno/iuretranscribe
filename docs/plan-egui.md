# IureTranscribe 2.0: interfaz egui y motor en un servicio aparte

> Plan, 2026-11-17. Decisión del dueño (S1.18 de IureDesk): «IureTranscribe con interfaz egui y
> whisper.cpp en un proceso aparte: ya». Se hace por fases; la app Tauri sigue publicándose hasta
> que la 2.0 tenga paridad en lo que la gente usa.

## Por qué

- **Una sola pila en la suite**: Rust + egui 0.37 + `iurefficient-ui` (temas, densidades, paleta,
  AccessKit), como IureDesk e IureProject. Sin webview, sin Node ni Svelte en el producto.
- **El motor C++ aislado**: whisper.cpp (y la diarización) viven en un proceso aparte. Si aborta
  (`panic = "abort"`, un driver de GPU que falla), la ventana no se cae: lo dice y reintenta.
- **Un solo grabador para todos**: hoy, si IureDesk graba por `--mcp` y la ventana también,
  graban los dos (S1.4). Con el servicio, la ventana, IureDesk y cualquier cliente MCP hablan con
  el mismo proceso: una grabación a la vez, visible para todos.
- **Editar la transcripción** con el editor de bloques de la suite (`DocEditor` de `iure-doc`,
  S1.11–S1.12 de IureDesk) cuando esté listo.

## Arquitectura

```text
┌──────────────────────┐      socket local (JSON-RPC)       ┌──────────────────────────┐
│ iuretranscribe (egui)│ ◀────────────────────────────────▶ │ iuretranscribe-service   │
│  interfaz, Rust puro │                                    │  instancia única          │
└──────────────────────┘                                    │  · grabador (PipeWire/    │
┌──────────────────────┐      el mismo socket               │    cpal)                  │
│ IureDesk             │ ◀────────────────────────────────▶ │  · whisper.cpp, GPU       │
└──────────────────────┘                                    │  · diarización            │
┌──────────────────────┐   stdio MCP (adaptador delgado)    │  · trabajos y su avance   │
│ Claude / Copilot     │ ◀── iuretranscribe --mcp ────────▶ │  · modelos (descarga)     │
└──────────────────────┘                                    └──────────────────────────┘
```

- **`iuretranscribe-service`**: binario sin ventana, instancia única por usuario (archivo de
  bloqueo + socket en la carpeta de ejecución; `0600` en Unix, ACL en Windows; token por sesión en
  un archivo privado, según el contrato común de la suite, `iuredesk/docs/integraciones.md` §0).
  Arranca bajo demanda (lo lanza la interfaz o el primer cliente) y se cierra solo tras un rato
  sin clientes ni trabajos ni grabación.
- **Protocolo**: las mismas herramientas que hoy expone `--mcp` (`transcribe_file`,
  `transcription_status`, `read_transcript`, `audio_info`, `list_recordings`,
  `transcription_setup`, `start_recording`, `recording_status`, `pause_recording`,
  `stop_recording`) más las que necesita la interfaz (modelos, ajustes, diarización, documentos,
  Iurefficient). Los segmentos en vivo y el avance llegan como **notificaciones** por el socket.
- **`iuretranscribe --mcp`** pasa a ser un adaptador: reenvía al servicio (lo arranca si hace
  falta). Mismas herramientas, sin cambios para Claude/Copilot ni para IureDesk.
- **Crates** (workspace):
  - `iuretranscribe-core` (Rust puro): ajustes, trabajos, formatos (SRT/VTT/TXT/JSON), vocabulario,
    documentos, protocolo; sin whisper.
  - `iuretranscribe-engine`: whisper-rs, GPU, diarización, grabador (lo único con C/C++).
  - `iuretranscribe-service`: el binario del servicio (usa `engine`).
  - `iuretranscribe-ui` (egui) y la app `iuretranscribe` (Rust puro: no enlaza `engine`).

## Lo que hay que portar (interfaz Svelte → egui)

| Vista actual | Líneas | En la 2.0 |
|---|---:|---|
| `SettingsView` | 685 | Ajustes por secciones (`iurefficient-ui`): modelo, idioma, GPU, salida, grabación, Iurefficient, agentes |
| `JobDetail` | 575 | Detalle del trabajo: texto con tiempos y hablantes (después `DocEditor` con bloques de segmento), resumen/minuta, exportar, subir a Iurefficient |
| `IurePicker` | 281 | Elegir caso/CRM para subir (por `iurefficient-connect`) |
| `RecordView` | 279 | Grabar: dispositivos, niveles, pausa, transcripción en vivo |
| `Sidebar` | 164 | Riel y lista de trabajos (lista virtual densa de `iurefficient-ui`) |
| `TranscribeView` | 128 | Soltar o elegir archivos, opciones |
| `ModelsView` | 122 | Modelos: descargar, cancelar, borrar |
| `JobCard`, `MetaForm`, `Toasts`, `Icon` | 200 | Componentes del ui-kit |

Los ~60 comandos de Tauri de `src-tauri/src/lib.rs` se reparten: los de motor/grabación/modelos
van al servicio; los de Iurefficient, apps y agentes quedan en la interfaz (por
`iurefficient-connect`), como en IureDesk.

## Fases

1. **Servicio** (sin tocar la app Tauri): extraer `core` y `engine`; `iuretranscribe-service` con
   el socket, instancia única y las herramientas actuales; `--mcp` como adaptador. La app Tauri
   empieza a usar el servicio para grabar y transcribir (resuelve el «graban los dos» de S1.4).
   Pruebas: protocolo, instancia única, reconexión, un aborto del motor no tumba al cliente.
2. **Interfaz egui mínima**: transcribir archivos, ver trabajos y su texto, modelos, ajustes
   básicos. Captura sin pantalla (`ui_shot`) y pruebas `egui_kittest` como en IureDesk.
3. **Grabar y en vivo**: vista de grabación con niveles, pausa y segmentos en vivo por
   notificaciones.
4. **Iurefficient y documentos**: subir a casos/CRM, resumen/minuta, compromisos.
5. **Edición con `DocEditor`** (cuando exista en `iure-doc`): la transcripción como documento con
   bloques de segmento (hablante, inicio/fin, clic → reproducir), sin perder la alineación.
6. **Paridad y retiro de Tauri**: lista de paridad firmada por el dueño; empaquetado (AppImage,
   MSI, DMG notarizado) del binario egui + el servicio.

## Riesgos

- **Reproducción de audio** (clic en un segmento → oír ese punto): necesita un camino de salida de
  audio en Rust (cpal) en la interfaz o pedirle al servicio que reproduzca.
- **Arrastrar y soltar** de archivos y diálogos: `rfd` con portal XDG (como IureDesk).
- **Empaquetado de dos binarios** y su firma en macOS (el servicio también se firma y notariza).
- **Compatibilidad de datos**: `jobs.json`, ajustes y documentos actuales se leen tal cual
  (`core` hereda sus formatos y se prueban con archivos reales de la 1.x).
