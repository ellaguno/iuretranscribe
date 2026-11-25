# Política de privacidad de IureTranscribe / IureTranscribe privacy policy

*Última actualización / Last updated: 2026-11-25* · [Español](#español) · [English](#english)

## Español

IureTranscribe es una aplicación de escritorio que graba y transcribe audio y video con Whisper,
genera subtítulos y, si tú lo pides, un resumen y una minuta. La publica Eduardo Llaguno Velasco
(«nosotros»). Esta política explica qué datos maneja (incluido su modo de servidor MCP,
`IureTranscribe --mcp`).

**Resumen:** la transcripción se hace en tu equipo: el audio y el texto no salen de él salvo que
tú uses un servicio externo (OpenRouter o tu instancia de Iurefficient). IureTranscribe no tiene
telemetría, analítica ni reportes automáticos de fallos, y no nos envía datos tuyos.

### Micrófono y audio del sistema
Sólo cuando pulsas *Grabar* (o un asistente lo pide por MCP con la app instalada), IureTranscribe
graba el micrófono y, si lo eliges, el audio que suena en el equipo. La grabación se guarda como
archivo en tu carpeta de Música (o en la que elijas) y se transcribe en el equipo.

### Qué sale de tu equipo
1. **Descarga de modelos.** Los modelos de Whisper se descargan de `huggingface.co` y los de
   identificación de hablantes de `huggingface.co` y `github.com`. No se envían datos tuyos; esos
   servicios reciben, como en cualquier conexión, tu dirección IP.
2. **Resumen y minuta con OpenRouter (opcional).** Si configuras tu propia llave de OpenRouter y
   pides un resumen o una minuta, el texto de la transcripción, las instrucciones y los datos que
   agregues (participantes, fecha…) se envían a `openrouter.ai`, que los pasa al modelo de IA que
   elegiste. Rigen las políticas de OpenRouter y del proveedor de ese modelo. Sin llave no se envía
   nada.
3. **Iurefficient (opcional).** Si inicias sesión, la app envía a *esa* instancia tu correo y
   contraseña (y el código de verificación en dos pasos, si lo usas) y puede pedir una contraseña
   de aplicación WebDAV o un token MCP. Cuando tú lo pides, sube transcripciones, documentos y,
   si lo activas, el audio o video original a tus carpetas, asuntos o CRM; registra horas; y usa
   el motor de IA de la instancia para minutas, resúmenes y compromisos. El tratamiento de esos
   datos lo rige la política de privacidad de quien opera esa instancia (normalmente tu
   organización).
4. **GitHub.** Para el panel «Apps de Iurefficient» y, fuera de la Microsoft Store, para avisar de
   versiones nuevas y actualizarse, la app consulta `api.github.com` y `github.com`. No envía datos
   tuyos. En la versión de la Microsoft Store la app no busca versiones nuevas de sí misma (la
   tienda la actualiza).
5. **Enlaces que abres.** El sitio, la demo, el canal de YouTube y otros enlaces sólo abren tu
   navegador.

### Qué se guarda en tu equipo
- Las grabaciones, transcripciones, subtítulos, resúmenes y minutas, como archivos en las carpetas
  que elijas (por omisión, junto al audio o en tu carpeta de Música).
- En la carpeta de datos de tu usuario: los ajustes (incluida tu llave de OpenRouter, si la
  configuras), la lista de trabajos, el vocabulario, los modelos descargados y un registro técnico
  de las dos últimas ejecuciones (rutas de archivo y errores, sin el texto de tus transcripciones).
- Si inicias sesión en Iurefficient: la sesión y la contraseña de aplicación WebDAV (no tu
  contraseña) en el llavero del sistema (Credential Manager de Windows, Llavero de macOS o Secret
  Service en Linux), compartido con las demás apps de Iurefficient, y el dominio y el correo de la
  cuenta activa. Cerrar la sesión los borra.
- Si conectas Claude Desktop, la app agrega su propia entrada a la configuración de Claude Desktop
  para que pueda usar el servidor MCP local; desconectar la quita.

### Asistentes de IA (MCP)
Si conectas IureTranscribe con un asistente (Claude Desktop, VS Code u otro), ese asistente puede
pedirle que transcriba archivos, lea transcripciones o grabe, y recibe el texto resultante. Lo que
el asistente haga con ese texto lo rige su propia política.

### Niños
IureTranscribe es una herramienta de trabajo y no está dirigida a menores de 13 años.

### Tus opciones y contacto
Puedes usar IureTranscribe sin cuenta y sin llave de OpenRouter: la transcripción funciona igual.
Puedes borrar grabaciones, transcripciones y modelos en cualquier momento. Para preguntas sobre esta
política, abre un *issue* en <https://github.com/ellaguno/iuretranscribe/issues>. Si la política
cambia, la nueva versión se publica en esta misma página con su fecha.

## English

IureTranscribe is a desktop application that records and transcribes audio and video with Whisper,
generates subtitles and, if you ask for it, a summary and meeting minutes. It is published by
Eduardo Llaguno Velasco ("we"). This policy explains what data it handles (including its MCP server
mode, `IureTranscribe --mcp`).

**Summary:** transcription happens on your computer: audio and text do not leave it unless you use
an external service (OpenRouter or your Iurefficient instance). IureTranscribe has no telemetry,
analytics or automatic crash reports, and sends no data about you to us.

### Microphone and system audio
Only when you press *Record* (or an assistant asks for it over MCP with the app installed),
IureTranscribe records the microphone and, if you choose so, the audio playing on the computer. The
recording is saved as a file in your Music folder (or the one you choose) and transcribed locally.

### What leaves your computer
1. **Model downloads.** Whisper models are downloaded from `huggingface.co`, and the speaker
   identification models from `huggingface.co` and `github.com`. No data about you is sent; as with
   any connection, those services receive your IP address.
2. **Summary and minutes with OpenRouter (optional).** If you set your own OpenRouter key and ask
   for a summary or minutes, the transcript text, the instructions and any details you add
   (participants, date…) are sent to `openrouter.ai`, which passes them to the AI model you chose.
   The policies of OpenRouter and of that model's provider apply. Without a key nothing is sent.
3. **Iurefficient (optional).** If you sign in, the app sends *that* instance your email and
   password (and the two-step verification code, if you use one) and may request a WebDAV app
   password or an MCP token. When you ask for it, it uploads transcripts, documents and, if
   enabled, the original audio or video to your folders, matters or CRM; logs hours; and uses the
   instance's AI engine for minutes, summaries and commitments. Those data are governed by the
   privacy policy of whoever runs that instance (usually your organization).
4. **GitHub.** For the "Iurefficient apps" panel and, outside the Microsoft Store, to announce new
   versions and update itself, the app queries `api.github.com` and `github.com`. It sends no data
   about you. In the Microsoft Store version the app does not check for new versions of itself (the
   Store updates it).
5. **Links you open.** The website, the demo, the YouTube channel and other links only open your
   browser.

### What is stored on your computer
- Recordings, transcripts, subtitles, summaries and minutes, as files in the folders you choose (by
  default, next to the audio or in your Music folder).
- In your user's data folder: the settings (including your OpenRouter key, if you set one), the job
  list, the vocabulary, the downloaded models and a technical log of the last two runs (file
  paths and errors, without the text of your transcripts).
- If you sign in to Iurefficient: the session and the WebDAV app password (not your password) in the
  system keychain (Windows Credential Manager, macOS Keychain or Secret Service on Linux), shared
  with the other Iurefficient apps, and the active account's domain and email. Signing out deletes
  them.
- If you connect Claude Desktop, the app adds its own entry to Claude Desktop's configuration so it
  can use the local MCP server; disconnecting removes it.

### AI assistants (MCP)
If you connect IureTranscribe to an assistant (Claude Desktop, VS Code or another), that assistant
can ask it to transcribe files, read transcripts or record, and receives the resulting text. What
the assistant does with that text is governed by its own policy.

### Children
IureTranscribe is a work tool and is not directed to children under 13.

### Your choices and contact
You can use IureTranscribe without an account and without an OpenRouter key: transcription works
the same. You can delete recordings, transcripts and models at any time. For questions about this
policy, open an issue at <https://github.com/ellaguno/iuretranscribe/issues>. If the policy changes,
the new version is published on this same page with its date.
