# IureTranscribe

Aplicación de escritorio (Linux, Windows y macOS) para transcribir audio y video
localmente con **Whisper** y, opcionalmente, generar **resumen** y **minuta**
con un modelo de lenguaje vía OpenRouter. Reemplaza al script `transcribe` de
línea de comandos con una interfaz gráfica.

- Transcripción 100 % local con [whisper.cpp](https://github.com/ggerganov/whisper.cpp).
  El instalador incluye el modelo Base para funcionar de inmediato; los demás
  modelos GGML (Large v3 Turbo recomendado) se descargan desde la propia app.
- Cola de archivos con arrastrar y soltar, progreso en vivo y segmentos en tiempo real.
- Salida en SRT, VTT, TXT y JSON, junto al archivo original o en una carpeta fija.
- Resumen y minuta en Markdown (OpenRouter, con instrucciones editables), con
  detalles de la reunión (participantes, fecha, lugar, notas) capturables en
  cualquier momento.
- Grabación integrada del micrófono y del audio del sistema (videollamadas,
  reuniones en el navegador) con transcripción en vivo y, si ambas fuentes están
  activas, **quién habló**: cada fuente se transcribe por separado y los segmentos
  llevan tu nombre o «Interlocutor».
- Con la minuta generada por Iurefficient, **compromisos → tareas**: la app lee los
  compromisos que detecta la instancia, permite corregir responsable y fecha, y los
  crea como tareas del proyecto.
- Aceleración por GPU según la variante: CUDA (NVIDIA, Linux/Windows), Vulkan
  (AMD, Intel o NVIDIA, Linux/Windows) o Metal (Apple Silicon). La variante CPU
  funciona en cualquier equipo.
- Decodifica MP3, WAV, M4A/AAC, MP4, MOV, MKV, FLAC y OGG sin dependencias; si
  `ffmpeg` está instalado, también Opus, WebM y otros formatos.

## Conexión con Iurefficient

En Ajustes → Cuenta de Iurefficient se indica el dominio de la instancia, el correo
y una **contraseña de aplicación** (`iurdav_…`, la misma que usa IureDav; la genera
cada usuario en su perfil y requiere que un administrador tenga WebDAV activado).
Con la cuenta conectada, cada archivo transcrito tiene el botón **Guardar en
Iurefficient**: se elige la carpeta del cliente y proyecto (o `General`) y se suben
la transcripción (SRT/TXT/…), el resumen, la minuta y, opcionalmente, el audio o
video original. Un archivo con el mismo nombre se guarda como versión nueva, no como
duplicado, igual que en la aplicación web. Las instancias no admiten `.srt`/`.vtt`
por defecto: en ese caso se suben como `.txt` (p. ej. `reunion.srt.txt`).

Además, con **sesión iniciada** (tu contraseña normal de Iurefficient, con verificación
en dos pasos si la tienes; la sesión se guarda en el llavero y dura 30 días) el mismo
botón ofrece dos destinos más:

- **Proyecto**: busca el proyecto o caso, sube los archivos como documentos del
  expediente y, si quieres, registra las horas de la reunión como tiempo facturable.
  Después, en la pestaña Minuta, «Minuta con el motor de Iurefficient» lista los
  formatos (blueprints) del despacho y genera el documento en la instancia, con los
  asistentes y detalles capturados, sin necesidad de llave de OpenRouter.
- **CRM**: adjunta los archivos a una oportunidad o lead y registra una actividad
  de reunión con la duración y el resumen.

El conector es el crate común [`iurefficient-connect`](https://github.com/ellaguno/iurefficient-connect),
compartido con IureEditor e IureDav; las credenciales viven en el llavero del sistema
(la misma entrada de contraseña WebDAV que usa IureDav), no en el archivo de ajustes.

## Grabación

| Plataforma | Micrófono | Audio del sistema (bocina) |
| --- | --- | --- |
| Linux | PipeWire (`pw-record`) o PulseAudio (`parec`) | Monitor de la salida predeterminada |
| Windows | WASAPI (cpal) | Loopback WASAPI de la salida predeterminada |
| macOS | CoreAudio (cpal) | Requiere un dispositivo virtual (p. ej. BlackHole) elegido como micrófono |

Las grabaciones se guardan como WAV mono de 16 kHz en `Música/IureTranscribe`
(configurable) y, si así se indica, se agregan a la cola y se transcriben solas.

## Stack

| Capa | Tecnología |
| --- | --- |
| Shell de escritorio | [Tauri 2](https://tauri.app) (Rust + WebView del sistema) |
| Motor de transcripción | whisper.cpp vía [`whisper-rs`](https://crates.io/crates/whisper-rs) |
| Decodificación de audio | [`symphonia`](https://crates.io/crates/symphonia) + remuestreo propio, con fallback a `ffmpeg` |
| Interfaz | Svelte 5 + Vite, TypeScript |
| Resumen/minuta | OpenRouter (API compatible con OpenAI) vía `reqwest` |

## Desarrollo

Requisitos: Node 22+, Rust estable, CMake, un compilador C++ y **libclang**
(lo usa `bindgen` para generar los bindings de whisper.cpp). En Linux además
los [prerrequisitos de Tauri](https://tauri.app/start/prerequisites/).

```bash
# Ubuntu / Debian
sudo apt install build-essential cmake libclang-dev libwebkit2gtk-4.1-dev librsvg2-dev patchelf libssl-dev
# Sin sudo: pip install --user libclang  y  export LIBCLANG_PATH=$(python3 -c 'import clang.native,os;print(os.path.dirname(clang.native.__file__))')
# macOS: xcode-select --install && brew install cmake llvm
# Windows: Visual Studio Build Tools (C++), CMake y LLVM (winget install LLVM.LLVM)
```

```bash
npm install
npm run tauri dev                      # CPU
npm run tauri dev -- --features cuda   # NVIDIA (requiere CUDA Toolkit / nvcc)
npm run tauri dev -- --features metal  # macOS Apple Silicon / Intel con Metal
npm run tauri dev -- --features vulkan # AMD / Intel / NVIDIA vía Vulkan (requiere Vulkan SDK)
```

Instaladores:

```bash
npm run tauri build -- --features cuda   # .deb/.rpm/.AppImage, .msi/.exe o .dmg según el sistema
```

Las features de GPU son de compilación: el binario resultante muestra el
backend en la barra lateral y en Ajustes. Si la GPU falla en tiempo de
ejecución, desactiva «Usar GPU» en Ajustes para caer a CPU.

## Dónde guarda las cosas

| Qué | Linux | Windows | macOS |
| --- | --- | --- | --- |
| Modelos | `~/.local/share/com.iurefficient.iuretranscribe/models` | `%APPDATA%\com.iurefficient.iuretranscribe\models` | `~/Library/Application Support/com.iurefficient.iuretranscribe/models` |
| Ajustes | `~/.config/com.iurefficient.iuretranscribe/settings.json` | `%APPDATA%\com.iurefficient.iuretranscribe\settings.json` | `~/Library/Application Support/com.iurefficient.iuretranscribe/settings.json` |

En Linux, la primera vez importa `OPENROUTER_API_KEY` y `OPENROUTER_MODEL` de
`~/.config/transcribe/config` si existen (el archivo del script original).

## CI

`.github/workflows/build.yml` compila en cada push y, al crear una etiqueta
`vX.Y.Z`, publica una release con instaladores para Linux (CPU, CUDA y Vulkan),
Windows (CPU, CUDA y Vulkan) y macOS (Apple Silicon con Metal e Intel).

| Variante | GPU | Tamaño aproximado |
| --- | --- | --- |
| `linux-x64`, `windows-x64` | ninguna (CPU) | 7–30 MB |
| `linux-x64-vulkan`, `windows-x64-vulkan` | AMD, Intel, NVIDIA (Vulkan) | 10–30 MB |
| `linux-x64-cuda`, `windows-x64-cuda` | NVIDIA (CUDA 12) | 450–600 MB (incluye cuBLAS) |
| `macos-arm64` | Apple Silicon (Metal) | 4 MB |
| `macos-x64` | ninguna (CPU) | 4 MB |
Los artefactos de cada push (sin etiqueta) quedan en la pestaña Actions del repositorio.
Las variantes CUDA incluyen el runtime de CUDA 12 (`cudart`, `cublas`, `cublasLt`)
dentro del instalador, así que sólo necesitan el driver de NVIDIA; `cublasLt` es
lo que ocupa casi todo el tamaño y no se puede omitir. En Linux CUDA se
distribuye sólo como `.deb` (el AppImage no puede empaquetar esas librerías).
Para un instalador ligero con GPU, usa la variante Vulkan.

## Licencia

MIT.
