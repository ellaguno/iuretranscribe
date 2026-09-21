<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { app, refreshApps, refreshIureSession, saveSettings, toast } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";

  let s = $derived(app.settings!);
  let showKey = $state(false);
  let showIurePass = $state(false);
  let iureTesting = $state(false);
  let loginPassword = $state("");
  let loginTotp = $state("");
  let totpToken = $state<string | null>(null);
  let loggingIn = $state(false);
  async function login() {
    loggingIn = true;
    clearTimeout(saveTimer);
    await saveSettings({});
    try {
      const r = await api.iureLogin(loginPassword, totpToken ? loginTotp : undefined, totpToken ?? undefined);
      if (r.requiresTotp) {
        totpToken = r.totpToken;
        toast("Introduce el código de verificación en dos pasos", "info");
      } else if (r.loggedIn) {
        loginPassword = "";
        loginTotp = "";
        totpToken = null;
        await refreshIureSession();
        toast(`Sesión iniciada${r.name ? ` como ${r.name}` : ""}`, "success");
      }
    } catch (e) {
      toast(String(e), "error", 8000);
    } finally {
      loggingIn = false;
    }
  }
  async function logout() {
    await api.iureLogout();
    await refreshIureSession();
  }
  let iureResult = $state<{ ok: boolean; text: string } | null>(null);
  async function testIure() {
    iureTesting = true;
    iureResult = null;
    clearTimeout(saveTimer);
    await saveSettings({});
    try {
      const info = await api.iureTestConnection();
      iureResult = { ok: true, text: `Conectado a ${info.webUrl}. Carpetas: ${info.rootFolders.join(", ") || "(ninguna)"}` };
    } catch (e) {
      iureResult = { ok: false, text: String(e) };
    } finally {
      iureTesting = false;
    }
  }
  const formats = [
    ["srt", "SRT (subtítulos)"],
    ["vtt", "VTT (web)"],
    ["txt", "TXT (texto plano)"],
    ["json", "JSON (segmentos)"],
  ] as const;
  const llmSuggestions = [
    "google/gemini-2.5-flash",
    "google/gemini-2.5-pro",
    "anthropic/claude-sonnet-4.5",
    "openai/gpt-4.1-mini",
    "openai/gpt-5-mini",
    "meta-llama/llama-3.3-70b-instruct",
  ];

  function toggleFormat(f: string) {
    const set = new Set(s.formats);
    if (set.has(f)) {
      if (set.size === 1) return toast("Debe quedar al menos un formato", "error", 2500);
      set.delete(f);
    } else set.add(f);
    saveSettings({ formats: [...set] });
  }
  async function pickDir(defaultPath?: string) {
    const dir = await open({ directory: true, title: "Carpeta de salida", defaultPath });
    if (typeof dir === "string") saveSettings({ outputMode: "custom", outputDir: dir });
  }
  /** Unidades de IureDav montadas: guardar ahí deja la transcripción directamente en Iurefficient. */
  let davMounted = $derived(app.davMounts.filter((m) => m.mounted && m.writable));
  let appsLoading = $state(false);
  async function loadApps() {
    appsLoading = true;
    await refreshApps(true);
    appsLoading = false;
  }
  async function launch(id: "transcribe" | "editor" | "dav") {
    try {
      await api.launchApp(id);
    } catch (e) {
      toast(String(e), "error", 6000);
    }
  }
  onMount(() => {
    if (!app.apps || app.apps.some((a) => a.latestVersion === null)) void loadApps();
  });
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  function debounced(patch: Parameters<typeof saveSettings>[0]) {
    Object.assign(s, patch);
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => saveSettings({}), 400);
  }
  function resetPrompts() {
    saveSettings({ summaryPrompt: "", minutesPrompt: "" }).then(async () => {
      app.settings = await api.getSettings();
    });
  }
</script>

<header class="top">
  <div>
    <h1>Ajustes</h1>
    <p class="hint">Los cambios se guardan automáticamente.</p>
  </div>
</header>

<div class="content scroll">
  <section class="card iure" id="iurefficient">
    <div class="iure-head">
      <h2><Icon name="cloud" size={17} /> Cuenta de Iurefficient</h2>
      {#if app.iureSession?.loggedIn}
        <span class="pill success"><Icon name="check" size={12} stroke={3} /> Conectado como {app.iureSession.name ?? app.iureSession.email}{app.iureSession.crm ? " · CRM" : ""}</span>
      {:else}
        <span class="pill">No conectado</span>
      {/if}
    </div>
    <p class="hint">Conecta tu cuenta para guardar la transcripción, el resumen y la minuta directo en un {app.iureSession?.terminology?.case ?? "proyecto"}, generar la minuta con el motor de Iurefficient (sin llave de OpenRouter), adjuntar a una oportunidad o lead del CRM y registrar horas. La contraseña no se guarda: sólo la sesión, en el llavero del sistema, y dura 30 días.</p>
    <div class="grid2">
      <div class="field">
        <label for="iure-domain">Dominio de la instancia</label>
        <input id="iure-domain" class="input" placeholder="p. ej. 2.ds.iurefficient.com" value={s.iureDomain} oninput={(e) => debounced({ iureDomain: (e.target as HTMLInputElement).value.trim() })} spellcheck="false" disabled={app.iureSession?.loggedIn} />
      </div>
      <div class="field">
        <label for="iure-email">Correo de usuario</label>
        <input id="iure-email" class="input" type="email" placeholder="tu@despacho.com" value={s.iureEmail} oninput={(e) => debounced({ iureEmail: (e.target as HTMLInputElement).value.trim() })} spellcheck="false" disabled={app.iureSession?.loggedIn} />
      </div>
    </div>
    {#if app.iureSession?.loggedIn}
      <div class="switchrow">
        <div>
          <span class="label">Generar la minuta con Iurefficient al guardar en un {app.iureSession.terminology?.case ?? "proyecto"}</span>
          <p class="hint">Usa el formato de minuta sugerido por la instancia y descarga el documento junto a la transcripción.</p>
        </div>
        <button class="switch" class:on={s.iureAutoCompose} aria-label="Minuta automática con Iurefficient" onclick={() => saveSettings({ iureAutoCompose: !s.iureAutoCompose })}></button>
      </div>
      <div class="row"><button class="btn sm ghost" onclick={logout}><Icon name="x" size={14} /> Cerrar sesión</button></div>
    {:else}
      <div class="row">
        {#if totpToken}
          <input class="input login-input" placeholder="Código de verificación (6 dígitos)" bind:value={loginTotp} inputmode="numeric" autocomplete="one-time-code" onkeydown={(e) => e.key === "Enter" && login()} />
        {:else}
          <input class="input login-input" type="password" placeholder="Contraseña de Iurefficient" bind:value={loginPassword} autocomplete="current-password" onkeydown={(e) => e.key === "Enter" && login()} />
        {/if}
        <button class="btn primary" onclick={login} disabled={loggingIn || !s.iureDomain || !s.iureEmail || (totpToken ? loginTotp.length < 6 : !loginPassword)}>
          {#if loggingIn}<span class="spin"><Icon name="loader" size={15} /></span>{:else}<Icon name="key" size={15} />{/if} {totpToken ? "Verificar" : "Conectar"}
        </button>
      </div>
      {#if app.iureSession?.error && s.iureDomain && s.iureEmail}<p class="hint errmsg">{app.iureSession.error}</p>{/if}
    {/if}

    <details class="advanced">
      <summary>Opciones avanzadas: acceso WebDAV (destino «Carpeta», compartido con IureDav)</summary>
      <p class="hint">No hace falta con la sesión iniciada. Sirve para guardar en cualquier carpeta del árbol de documentos con una <strong>contraseña de aplicación</strong> (empieza con <code>iurdav_</code>) que generas en tu perfil de Iurefficient; requiere WebDAV activado por un administrador. Vacía el campo para eliminarla del llavero.</p>
      <div class="grid2">
        <div class="field">
          <label for="iure-pass">Contraseña de aplicación WebDAV</label>
          <div class="row">
            <input id="iure-pass" class="input" type={showIurePass ? "text" : "password"} placeholder="iurdav_…" value={s.iureAppPassword} oninput={(e) => debounced({ iureAppPassword: (e.target as HTMLInputElement).value.trim() })} autocomplete="off" spellcheck="false" />
            <button class="btn icon ghost" onclick={() => (showIurePass = !showIurePass)} title={showIurePass ? "Ocultar" : "Mostrar"}><Icon name={showIurePass ? "eyeOff" : "eye"} size={16} /></button>
          </div>
        </div>
        <div class="field">
          <div class="switchrow">
            <div>
              <span class="label">Incluir el audio o video original</span>
              <p class="hint">Al guardar, subir también la grabación además de los textos.</p>
            </div>
            <button class="switch" class:on={s.iureUploadMedia} aria-label="Incluir audio" onclick={() => saveSettings({ iureUploadMedia: !s.iureUploadMedia })}></button>
          </div>
        </div>
      </div>
      <div class="row">
        {#if app.iureSession?.loggedIn && !s.iureAppPassword}
          <button class="btn sm primary" onclick={async () => { try { await api.iureEnsureWebdavPassword(); app.settings = await api.getSettings(); toast("Contraseña de aplicación creada y guardada en el llavero", "success"); } catch (e) { toast(String(e), "error", 8000); } }}>
            <Icon name="key" size={14} /> Crear con mi sesión
          </button>
        {/if}
        <button class="btn sm" onclick={testIure} disabled={iureTesting || !s.iureDomain || !s.iureEmail || !s.iureAppPassword}>
          {#if iureTesting}<span class="spin"><Icon name="loader" size={15} /></span>{:else}<Icon name="check" size={15} />{/if} Probar acceso WebDAV
        </button>
        {#if iureResult}<span class="hint" class:okmsg={iureResult.ok} class:errmsg={!iureResult.ok}>{iureResult.text}</span>{/if}
      </div>
    </details>
  </section>

  <section class="card">
    <h2><Icon name="file" size={17} /> Salida</h2>
    <div class="grid2">
      <div class="field">
        <span class="label">Formatos que se generan</span>
        <div class="checks">
          {#each formats as [id, label]}
            <label class="check">
              <input type="checkbox" checked={s.formats.includes(id)} onchange={() => toggleFormat(id)} />
              {label}
            </label>
          {/each}
        </div>
      </div>
      <div class="field">
        <span class="label">Carpeta de salida</span>
        <label class="check"><input type="radio" name="out" checked={s.outputMode === "same"} onchange={() => saveSettings({ outputMode: "same" })} /> Junto al archivo original</label>
        <label class="check"><input type="radio" name="out" checked={s.outputMode === "custom"} onchange={() => (s.outputDir ? saveSettings({ outputMode: "custom" }) : pickDir())} /> Carpeta fija</label>
        {#if s.outputMode === "custom"}
          <div class="row">
            <span class="path" title={s.outputDir ?? ""}>{s.outputDir ?? "(sin elegir)"}</span>
            <button class="btn sm" onclick={() => pickDir()}><Icon name="folder" size={14} /> Cambiar</button>
          </div>
        {/if}
        {#if davMounted.length}
          <div class="davhint">
            <Icon name="drive" size={14} />
            <span class="hint">IureDav tiene montada la unidad {davMounted.map((m) => `«${m.name}»`).join(", ")}: si eliges una carpeta de proyecto dentro de ella, la transcripción queda directamente en Iurefficient.</span>
            {#each davMounted as m (m.id)}
              <button class="btn sm" title={m.mountPoint} onclick={() => pickDir(m.mountPoint)}><Icon name="folder" size={14} /> Elegir en {m.name}</button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </section>

  <section class="card">
    <h2><Icon name="cpu" size={17} /> Rendimiento</h2>
    <div class="grid2">
      <div class="field">
        <div class="switchrow">
          <div>
            <span class="label">Usar GPU</span>
            <p class="hint">Backend compilado: {app.sys?.backend}. {app.sys?.backend === "CPU" ? "Esta versión no incluye aceleración por GPU." : "Si falla la GPU, desactívalo para usar sólo CPU."}</p>
          </div>
          <button class="switch" class:on={s.useGpu} aria-label="Usar GPU" onclick={() => saveSettings({ useGpu: !s.useGpu })}></button>
        </div>
      </div>
      <div class="field">
        <label for="beam">Calidad de decodificación</label>
        <select id="beam" class="input" value={String(s.beamSize)} onchange={(e) => saveSettings({ beamSize: Number((e.target as HTMLSelectElement).value) })}>
          <option value="1">Rápida (greedy)</option>
          <option value="3">Equilibrada (beam 3)</option>
          <option value="5">Alta (beam 5, recomendada)</option>
          <option value="8">Máxima (beam 8, lenta)</option>
        </select>
      </div>
      <div class="field">
        <label for="threads">Hilos de CPU</label>
        <select id="threads" class="input" value={String(s.threads)} onchange={(e) => saveSettings({ threads: Number((e.target as HTMLSelectElement).value) })}>
          <option value="0">Automático</option>
          {#each Array.from({ length: app.sys?.cpuThreads ?? 4 }, (_, i) => i + 1) as n}
            <option value={String(n)}>{n}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <div class="switchrow">
          <div>
            <span class="label">Traducir al inglés</span>
            <p class="hint">Whisper sólo puede traducir hacia el inglés.</p>
          </div>
          <button class="switch" class:on={s.translate} aria-label="Traducir" onclick={() => saveSettings({ translate: !s.translate })}></button>
        </div>
      </div>
    </div>
    {#if app.sys && !app.sys.ffmpegAvailable}
      <p class="hint note"><Icon name="info" size={14} /> ffmpeg no está instalado. La app decodifica MP3, WAV, M4A/AAC, MP4, MKV, FLAC y OGG por sí sola; con ffmpeg podría abrir también Opus, WebM y otros.</p>
    {/if}
  </section>

  <section class="card">
    <h2><Icon name="sparkles" size={17} /> Resumen y minuta (OpenRouter)</h2>
    <p class="hint">Se envía el texto de la transcripción a OpenRouter para generar un resumen y una minuta en Markdown. Consigue una llave en openrouter.ai.</p>
    <div class="grid2">
      <div class="field">
        <label for="key">Llave de API</label>
        <div class="row">
          <input id="key" class="input" type={showKey ? "text" : "password"} placeholder="sk-or-v1-…" value={s.openrouterApiKey} oninput={(e) => debounced({ openrouterApiKey: (e.target as HTMLInputElement).value.trim() })} autocomplete="off" spellcheck="false" />
          <button class="btn icon ghost" onclick={() => (showKey = !showKey)} title={showKey ? "Ocultar" : "Mostrar"}><Icon name={showKey ? "eyeOff" : "eye"} size={16} /></button>
        </div>
      </div>
      <div class="field">
        <label for="llm">Modelo de lenguaje</label>
        <input id="llm" class="input" list="llm-list" value={s.openrouterModel} oninput={(e) => debounced({ openrouterModel: (e.target as HTMLInputElement).value.trim() })} spellcheck="false" />
        <datalist id="llm-list">
          {#each llmSuggestions as m}<option value={m}></option>{/each}
        </datalist>
      </div>
      <div class="field">
        <div class="switchrow">
          <span class="label">Generar resumen automáticamente</span>
          <button class="switch" class:on={s.autoSummary} aria-label="Resumen automático" onclick={() => saveSettings({ autoSummary: !s.autoSummary })}></button>
        </div>
      </div>
      <div class="field">
        <div class="switchrow">
          <span class="label">Generar minuta automáticamente</span>
          <button class="switch" class:on={s.autoMinutes} aria-label="Minuta automática" onclick={() => saveSettings({ autoMinutes: !s.autoMinutes })}></button>
        </div>
      </div>
      <div class="field wide">
        <label for="sp">Instrucciones para el resumen</label>
        <textarea id="sp" class="input" value={s.summaryPrompt} oninput={(e) => debounced({ summaryPrompt: (e.target as HTMLTextAreaElement).value })}></textarea>
      </div>
      <div class="field wide">
        <label for="mp">Instrucciones para la minuta</label>
        <textarea id="mp" class="input" value={s.minutesPrompt} oninput={(e) => debounced({ minutesPrompt: (e.target as HTMLTextAreaElement).value })}></textarea>
      </div>
    </div>
    <div><button class="btn ghost sm" onclick={resetPrompts}><Icon name="refresh" size={14} /> Restaurar instrucciones predeterminadas</button></div>
  </section>

  <section class="card">
    <div class="iure-head">
      <h2><Icon name="apps" size={17} /> Apps de Iurefficient</h2>
      <button class="btn sm ghost" onclick={loadApps} disabled={appsLoading}>{#if appsLoading}<span class="spin"><Icon name="loader" size={14} /></span>{:else}<Icon name="refresh" size={14} />{/if} Actualizar</button>
    </div>
    <p class="hint">Las tres apps de escritorio trabajan juntas: IureTranscribe transcribe, IureEditor edita y publica los documentos, IureDav monta los documentos de tu instancia como una unidad local. Comparten la sesión y las contraseñas de aplicación en el llavero del sistema.</p>
    {#if app.apps}
      <div class="apps">
        {#each app.apps as a (a.id)}
          <div class="appcard" class:me={a.id === "transcribe"}>
            <div class="apphead">
              <strong>{a.name}</strong>
              {#if a.id === "transcribe"}
                <span class="pill success">esta app · {app.sys?.version}</span>
              {:else if a.installed}
                <span class="pill success"><Icon name="check" size={11} stroke={3} /> instalada</span>
              {:else}
                <span class="pill">no instalada</span>
              {/if}
              {#if a.latestVersion}<span class="hint">última: {a.latestVersion}</span>{/if}
            </div>
            <p class="hint">{a.description}</p>
            <div class="row-actions">
              {#if a.id !== "transcribe"}
                {#if a.installed}
                  <button class="btn sm" onclick={() => launch(a.id)} title={a.path ?? ""}><Icon name="external" size={13} /> Abrir</button>
                {:else}
                  <button class="btn sm primary" onclick={() => openUrl(a.downloadUrl)}><Icon name="download" size={13} /> Descargar</button>
                {/if}
              {/if}
              <button class="btn sm ghost" onclick={() => openUrl(`https://github.com/${a.id === "transcribe" ? "ellaguno/iuretranscribe" : a.id === "editor" ? "ellaguno/iureditor" : "ellaguno/iuredav"}`)}><Icon name="globe" size={13} /> Código</button>
            </div>
          </div>
        {/each}
      </div>
      {#if app.davMounts.length}
        <p class="hint"><Icon name="drive" size={13} /> Unidades de IureDav: {app.davMounts.map((m) => `${m.name} → ${m.mountPoint}${m.mounted ? "" : " (sin montar)"}`).join(" · ")}</p>
      {/if}
    {:else}
      <p class="hint"><span class="spin"><Icon name="loader" size={13} /></span> Buscando apps instaladas…</p>
    {/if}
  </section>

  <section class="card">
    <h2><Icon name="sun" size={17} /> Apariencia</h2>
    <div class="seg">
      {#each [["system", "Sistema", "monitor"], ["light", "Claro", "sun"], ["dark", "Oscuro", "moon"]] as [id, label, icon]}
        <button class:active={s.theme === id} onclick={() => saveSettings({ theme: id as "system" | "light" | "dark" })}><Icon name={icon} size={15} /> {label}</button>
      {/each}
    </div>
    <div class="switchrow">
      <div>
        <span class="label">Avisar de versiones nuevas</span>
        <p class="hint">Al arrancar consulta las releases de GitHub. Versión instalada: {app.sys?.version} ({app.sys?.updateTarget}). Si la app se instaló como AppImage, en Windows o en macOS puede actualizarse sola; con .deb/.rpm sólo avisa.</p>
      </div>
      <button class="switch" class:on={s.checkUpdates} aria-label="Avisar de versiones nuevas" onclick={() => saveSettings({ checkUpdates: !s.checkUpdates })}></button>
    </div>
    <div><button class="btn sm" onclick={() => import("../lib/updater").then((m) => m.checkForUpdates(false))}><Icon name="refresh" size={14} /> Buscar actualizaciones ahora</button></div>
    <p class="hint">Modelos en: <code>{app.sys?.modelsDir}</code><br />Ajustes en: <code>{app.sys?.settingsPath}</code></p>
  </section>
</div>

<style>
  .top { padding: 22px 26px 14px; }
  .content { flex: 1; min-height: 0; padding: 0 26px 26px; display: flex; flex-direction: column; gap: 14px; }
  section { padding: 18px 20px; display: flex; flex-direction: column; gap: 14px; }
  section h2 { display: flex; align-items: center; gap: 8px; }
  .grid2 { display: grid; grid-template-columns: 1fr 1fr; gap: 16px 28px; }
  .wide { grid-column: 1 / -1; }
  .checks { display: flex; flex-wrap: wrap; gap: 6px 16px; }
  .check { display: flex; align-items: center; gap: 7px; font-size: 13.5px; cursor: pointer; }
  .check input { accent-color: var(--accent); }
  .row { display: flex; align-items: center; gap: 8px; }
  .path { flex: 1; font-size: 12.5px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .switchrow { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .note { display: flex; gap: 6px; align-items: flex-start; }
  .spin { display: inline-flex; animation: spin 1s linear infinite; }
  .okmsg { color: var(--success); }
  .iure { border-color: var(--accent); }
  .iure-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  .login-input { max-width: 340px; }
  .advanced { border-top: 1px solid var(--border); padding-top: 10px; display: flex; flex-direction: column; gap: 12px; }
  .advanced summary { cursor: pointer; color: var(--text-2); font-weight: 550; font-size: 13px; }
  .errmsg { color: var(--danger); }
  .seg { display: inline-flex; background: var(--surface-2); padding: 3px; border-radius: 10px; gap: 2px; width: fit-content; }
  .seg button { display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; border-radius: 8px; color: var(--text-2); font-weight: 550; }
  .seg button.active { background: var(--surface); color: var(--text); box-shadow: var(--shadow); }
  code { font-family: var(--mono); font-size: 12px; user-select: text; }
  .davhint { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 6px; padding: 8px 10px; border: 1px dashed var(--border); border-radius: 9px; color: var(--accent); }
  .davhint .hint { flex: 1; min-width: 200px; }
  .apps { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 10px; }
  .appcard { display: flex; flex-direction: column; gap: 6px; padding: 12px 14px; border: 1px solid var(--border); border-radius: 10px; background: var(--surface-2); }
  .appcard.me { border-color: var(--accent); }
  .apphead { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .row-actions { display: flex; gap: 6px; flex-wrap: wrap; }
</style>
