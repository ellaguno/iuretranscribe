<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type AppId, type Correction } from "../lib/api";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import { app, refreshApps, refreshIureSession, saveSettings, toast } from "../lib/state.svelte";
  import Icon from "./Icon.svelte";
  import { i18n, t, type Key } from "../lib/i18n.svelte";

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
        toast(t("settings.enterTotp"), "info");
      } else if (r.loggedIn) {
        loginPassword = "";
        loginTotp = "";
        totpToken = null;
        await refreshIureSession();
        toast(r.name ? t("settings.signedInAs", { name: r.name }) : t("settings.signedIn"), "success");
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
      iureResult = { ok: true, text: t("settings.connectedTo", { url: info.webUrl, folders: info.rootFolders.join(", ") || t("settings.noFolders") }) };
    } catch (e) {
      iureResult = { ok: false, text: String(e) };
    } finally {
      iureTesting = false;
    }
  }
  const formats: [string, Key][] = [
    ["srt", "settings.fmtSrt"],
    ["vtt", "settings.fmtVtt"],
    ["txt", "settings.fmtTxt"],
    ["json", "settings.fmtJson"],
  ];
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
      if (set.size === 1) return toast(t("settings.oneFormat"), "error", 2500);
      set.delete(f);
    } else set.add(f);
    saveSettings({ formats: [...set] });
  }
  async function pickDir(defaultPath?: string) {
    const dir = await open({ directory: true, title: t("settings.outputFolder"), defaultPath });
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
  const REPOS: Record<AppId, string> = { transcribe: "iuretranscribe", editor: "iureditor", dav: "iuredav", ocr: "iureocr" };
  async function launch(id: AppId) {
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
  function setCorrection(i: number, patch: Partial<Correction>) {
    debounced({ corrections: s.corrections.map((c, j) => (j === i ? { ...c, ...patch } : c)) });
  }
  function addCorrection() {
    saveSettings({ corrections: [...s.corrections, { wrong: "", right: "" }] });
  }
  function removeCorrection(i: number) {
    saveSettings({ corrections: s.corrections.filter((_, j) => j !== i) });
  }
  /** Instrucciones: vacío = predeterminadas del idioma; se muestran en el cuadro y, si el
   *  usuario deja el texto igual al predeterminado, se guarda vacío para seguir al idioma. */
  function setPrompt(key: "summaryPrompt" | "minutesPrompt", value: string) {
    const def = key === "summaryPrompt" ? app.defaultPrompts.summary : app.defaultPrompts.minutes;
    debounced({ [key]: value.trim() === def.trim() ? "" : value });
  }
  const uiLanguages: ["auto" | "en" | "es", string][] = [
    ["auto", ""],
    ["en", "English"],
    ["es", "Español"],
  ];
  function resetPrompts() {
    saveSettings({ summaryPrompt: "", minutesPrompt: "" }).then(async () => {
      app.settings = await api.getSettings();
    });
  }
</script>

<header class="top">
  <div>
    <h1>{t("settings.title")}</h1>
    <p class="hint">{t("settings.autosave")}</p>
  </div>
</header>

<div class="content scroll">
  <section class="card">
    <h2><Icon name="globe" size={17} /> {t("settings.uiLanguage")}</h2>
    <div class="seg">
      {#each uiLanguages as [id, label]}
        <button class:active={(s.uiLanguage ?? "auto") === id} onclick={() => saveSettings({ uiLanguage: id })}>{id === "auto" ? t("settings.uiAuto") : label}</button>
      {/each}
    </div>
  </section>

  <section class="card iure" id="iurefficient">
    <div class="iure-head">
      <h2><Icon name="cloud" size={17} /> {t("settings.account")}</h2>
      {#if app.iureSession?.loggedIn}
        <span class="pill success"><Icon name="check" size={12} stroke={3} /> {t("settings.connectedAsPill", { name: app.iureSession.name ?? app.iureSession.email ?? "" })}{app.iureSession.crm ? " · CRM" : ""}</span>
      {:else}
        <span class="pill">{t("settings.notConnected")}</span>
      {/if}
    </div>
    <p class="hint">{t("settings.accountHint", { case: app.iureSession?.terminology?.case ?? t("term.case") })}</p>
    <div class="grid2">
      <div class="field">
        <label for="iure-domain">{t("settings.domain")}</label>
        <input id="iure-domain" class="input" placeholder={t("settings.domainPh")} value={s.iureDomain} oninput={(e) => debounced({ iureDomain: (e.target as HTMLInputElement).value.trim() })} spellcheck="false" disabled={app.iureSession?.loggedIn} />
      </div>
      <div class="field">
        <label for="iure-email">{t("settings.email")}</label>
        <input id="iure-email" class="input" type="email" placeholder={t("settings.emailPh")} value={s.iureEmail} oninput={(e) => debounced({ iureEmail: (e.target as HTMLInputElement).value.trim() })} spellcheck="false" disabled={app.iureSession?.loggedIn} />
      </div>
    </div>
    {#if app.iureSession?.loggedIn}
      <div class="switchrow">
        <div>
          <span class="label">{t("settings.autoCompose", { case: app.iureSession.terminology?.case ?? t("term.case") })}</span>
          <p class="hint">{t("settings.autoComposeHint")}</p>
        </div>
        <button class="switch" class:on={s.iureAutoCompose} aria-label={t("settings.autoComposeAria")} onclick={() => saveSettings({ iureAutoCompose: !s.iureAutoCompose })}></button>
      </div>
      <div class="row"><button class="btn sm ghost" onclick={logout}><Icon name="x" size={14} /> {t("settings.logout")}</button></div>
    {:else}
      <div class="row">
        {#if totpToken}
          <input class="input login-input" placeholder={t("settings.totpPh")} bind:value={loginTotp} inputmode="numeric" autocomplete="one-time-code" onkeydown={(e) => e.key === "Enter" && login()} />
        {:else}
          <input class="input login-input" type="password" placeholder={t("settings.passwordPh")} bind:value={loginPassword} autocomplete="current-password" onkeydown={(e) => e.key === "Enter" && login()} />
        {/if}
        <button class="btn primary" onclick={login} disabled={loggingIn || !s.iureDomain || !s.iureEmail || (totpToken ? loginTotp.length < 6 : !loginPassword)}>
          {#if loggingIn}<span class="spin"><Icon name="loader" size={15} /></span>{:else}<Icon name="key" size={15} />{/if} {totpToken ? t("settings.verify") : t("settings.connect")}
        </button>
      </div>
      {#if app.iureSession?.error && s.iureDomain && s.iureEmail}<p class="hint errmsg">{app.iureSession.error}</p>{/if}
    {/if}

    <details class="advanced">
      <summary>{t("settings.advanced")}</summary>
      <p class="hint">{t("settings.advancedHint1")} <strong>{t("settings.advancedHint2")}</strong> {t("settings.advancedHint3")} <code>iurdav_</code>{t("settings.advancedHint4")}</p>
      <div class="grid2">
        <div class="field">
          <label for="iure-pass">{t("settings.webdavPassword")}</label>
          <div class="row">
            <input id="iure-pass" class="input" type={showIurePass ? "text" : "password"} placeholder="iurdav_…" value={s.iureAppPassword} oninput={(e) => debounced({ iureAppPassword: (e.target as HTMLInputElement).value.trim() })} autocomplete="off" spellcheck="false" />
            <button class="btn icon ghost" onclick={() => (showIurePass = !showIurePass)} title={showIurePass ? t("settings.hide") : t("settings.show")}><Icon name={showIurePass ? "eyeOff" : "eye"} size={16} /></button>
          </div>
        </div>
        <div class="field">
          <div class="switchrow">
            <div>
              <span class="label">{t("settings.includeMedia")}</span>
              <p class="hint">{t("settings.includeMediaHint")}</p>
            </div>
            <button class="switch" class:on={s.iureUploadMedia} aria-label={t("settings.includeMediaAria")} onclick={() => saveSettings({ iureUploadMedia: !s.iureUploadMedia })}></button>
          </div>
        </div>
      </div>
      <div class="row">
        {#if app.iureSession?.loggedIn && !s.iureAppPassword}
          <button class="btn sm primary" onclick={async () => { try { await api.iureEnsureWebdavPassword(); app.settings = await api.getSettings(); toast(t("settings.passwordCreated"), "success"); } catch (e) { toast(String(e), "error", 8000); } }}>
            <Icon name="key" size={14} /> {t("settings.createWithSession")}
          </button>
        {/if}
        <button class="btn sm" onclick={testIure} disabled={iureTesting || !s.iureDomain || !s.iureEmail || !s.iureAppPassword}>
          {#if iureTesting}<span class="spin"><Icon name="loader" size={15} /></span>{:else}<Icon name="check" size={15} />{/if} {t("settings.testWebdav")}
        </button>
        {#if iureResult}<span class="hint" class:okmsg={iureResult.ok} class:errmsg={!iureResult.ok}>{iureResult.text}</span>{/if}
      </div>
    </details>
  </section>

  <section class="card">
    <h2><Icon name="file" size={17} /> {t("settings.output")}</h2>
    <div class="grid2">
      <div class="field">
        <span class="label">{t("settings.formats")}</span>
        <div class="checks">
          {#each formats as [id, label]}
            <label class="check">
              <input type="checkbox" checked={s.formats.includes(id)} onchange={() => toggleFormat(id)} />
              {t(label)}
            </label>
          {/each}
        </div>
      </div>
      <div class="field">
        <span class="label">{t("settings.outputFolder")}</span>
        <label class="check"><input type="radio" name="out" checked={s.outputMode === "same"} onchange={() => saveSettings({ outputMode: "same" })} /> {t("settings.sameFolder")}</label>
        <label class="check"><input type="radio" name="out" checked={s.outputMode === "custom"} onchange={() => (s.outputDir ? saveSettings({ outputMode: "custom" }) : pickDir())} /> {t("settings.fixedFolder")}</label>
        {#if s.outputMode === "custom"}
          <div class="row">
            <span class="path" title={s.outputDir ?? ""}>{s.outputDir ?? t("settings.notChosen")}</span>
            <button class="btn sm" onclick={() => pickDir()}><Icon name="folder" size={14} /> {t("settings.change")}</button>
          </div>
        {/if}
        {#if davMounted.length}
          <div class="davhint">
            <Icon name="drive" size={14} />
            <span class="hint">{t("settings.davHint", { drives: davMounted.map((m) => (i18n.lang === "es" ? `«${m.name}»` : `“${m.name}”`)).join(", ") })}</span>
            {#each davMounted as m (m.id)}
              <button class="btn sm" title={m.mountPoint} onclick={() => pickDir(m.mountPoint)}><Icon name="folder" size={14} /> {t("settings.chooseIn", { name: m.name })}</button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </section>

  <section class="card">
    <h2><Icon name="edit" size={17} /> {t("settings.vocabTitle")}</h2>
    <div class="field">
      <label for="vocab">{t("settings.vocab")}</label>
      <textarea id="vocab" class="input vocab" placeholder={t("settings.vocabPh")} value={s.vocabulary} oninput={(e) => debounced({ vocabulary: (e.target as HTMLTextAreaElement).value })}></textarea>
      <p class="hint">{t("settings.vocabHint")}</p>
    </div>
    <div class="field">
      <span class="label">{t("settings.corrections")}</span>
      <p class="hint">{t("settings.correctionsHint")}</p>
      {#if s.corrections.length}
        <div class="corr">
          <span class="hint">{t("settings.howItComesOut")}</span><span class="hint">{t("settings.howItShouldBe")}</span><span></span>
          {#each s.corrections as c, i (i)}
            <input class="input" placeholder="Yaguno, Llagunno" value={c.wrong} oninput={(e) => setCorrection(i, { wrong: (e.target as HTMLInputElement).value })} spellcheck="false" />
            <input class="input" placeholder="Llaguno" value={c.right} oninput={(e) => setCorrection(i, { right: (e.target as HTMLInputElement).value })} spellcheck="false" />
            <button class="btn ghost sm" aria-label={t("settings.removeCorrection")} title={t("settings.remove")} onclick={() => removeCorrection(i)}><Icon name="trash" size={14} /></button>
          {/each}
        </div>
      {/if}
      <div><button class="btn sm" onclick={addCorrection}><Icon name="plus" size={14} /> {t("settings.addCorrection")}</button></div>
    </div>
  </section>

  <section class="card">
    <h2><Icon name="cpu" size={17} /> {t("settings.performance")}</h2>
    <div class="grid2">
      <div class="field">
        <div class="switchrow">
          <div>
            <span class="label">{t("settings.useGpu")}</span>
            <p class="hint">{t("settings.backend", { backend: app.sys?.backend ?? "" })} {app.sys?.backend === "CPU" ? t("settings.noGpuBuild") : t("settings.gpuFallback")}</p>
          </div>
          <button class="switch" class:on={s.useGpu} aria-label={t("settings.useGpu")} onclick={() => saveSettings({ useGpu: !s.useGpu })}></button>
        </div>
      </div>
      <div class="field">
        <label for="beam">{t("settings.quality")}</label>
        <select id="beam" class="input" value={String(s.beamSize)} onchange={(e) => saveSettings({ beamSize: Number((e.target as HTMLSelectElement).value) })}>
          <option value="1">{t("settings.beam1")}</option>
          <option value="3">{t("settings.beam3")}</option>
          <option value="5">{t("settings.beam5")}</option>
          <option value="8">{t("settings.beam8")}</option>
        </select>
      </div>
      <div class="field">
        <label for="threads">{t("settings.threads")}</label>
        <select id="threads" class="input" value={String(s.threads)} onchange={(e) => saveSettings({ threads: Number((e.target as HTMLSelectElement).value) })}>
          <option value="0">{t("settings.auto")}</option>
          {#each Array.from({ length: app.sys?.cpuThreads ?? 4 }, (_, i) => i + 1) as n}
            <option value={String(n)}>{n}</option>
          {/each}
        </select>
      </div>
      <div class="field">
        <div class="switchrow">
          <div>
            <span class="label">{t("settings.translate")}</span>
            <p class="hint">{t("settings.translateHint")}</p>
          </div>
          <button class="switch" class:on={s.translate} aria-label={t("settings.translateAria")} onclick={() => saveSettings({ translate: !s.translate })}></button>
        </div>
      </div>
    </div>
    {#if app.sys && !app.sys.ffmpegAvailable}
      <p class="hint note"><Icon name="info" size={14} /> {t("settings.noFfmpeg")}</p>
    {/if}
  </section>

  <section class="card">
    <h2><Icon name="sparkles" size={17} /> {t("settings.llmTitle")}</h2>
    <p class="hint">{t("settings.llmHint")}</p>
    <div class="grid2">
      <div class="field">
        <label for="key">{t("settings.apiKey")}</label>
        <div class="row">
          <input id="key" class="input" type={showKey ? "text" : "password"} placeholder="sk-or-v1-…" value={s.openrouterApiKey} oninput={(e) => debounced({ openrouterApiKey: (e.target as HTMLInputElement).value.trim() })} autocomplete="off" spellcheck="false" />
          <button class="btn icon ghost" onclick={() => (showKey = !showKey)} title={showKey ? t("settings.hide") : t("settings.show")}><Icon name={showKey ? "eyeOff" : "eye"} size={16} /></button>
        </div>
      </div>
      <div class="field">
        <label for="llm">{t("settings.llmModel")}</label>
        <input id="llm" class="input" list="llm-list" value={s.openrouterModel} oninput={(e) => debounced({ openrouterModel: (e.target as HTMLInputElement).value.trim() })} spellcheck="false" />
        <datalist id="llm-list">
          {#each llmSuggestions as m}<option value={m}></option>{/each}
        </datalist>
      </div>
      <div class="field">
        <div class="switchrow">
          <span class="label">{t("settings.autoSummary")}</span>
          <button class="switch" class:on={s.autoSummary} aria-label={t("settings.autoSummaryAria")} onclick={() => saveSettings({ autoSummary: !s.autoSummary })}></button>
        </div>
      </div>
      <div class="field">
        <div class="switchrow">
          <span class="label">{t("settings.autoMinutes")}</span>
          <button class="switch" class:on={s.autoMinutes} aria-label={t("settings.autoMinutesAria")} onclick={() => saveSettings({ autoMinutes: !s.autoMinutes })}></button>
        </div>
      </div>
      <div class="field wide">
        <label for="sp">{t("settings.summaryPrompt")}</label>
        <textarea id="sp" class="input" value={s.summaryPrompt || app.defaultPrompts.summary} oninput={(e) => setPrompt("summaryPrompt", (e.target as HTMLTextAreaElement).value)}></textarea>
      </div>
      <div class="field wide">
        <label for="mp">{t("settings.minutesPrompt")}</label>
        <textarea id="mp" class="input" value={s.minutesPrompt || app.defaultPrompts.minutes} oninput={(e) => setPrompt("minutesPrompt", (e.target as HTMLTextAreaElement).value)}></textarea>
      </div>
    </div>
    <div><button class="btn ghost sm" onclick={resetPrompts}><Icon name="refresh" size={14} /> {t("settings.resetPrompts")}</button></div>
  </section>

  <section class="card">
    <div class="iure-head">
      <h2><Icon name="apps" size={17} /> {t("settings.apps")}</h2>
      <button class="btn sm ghost" onclick={loadApps} disabled={appsLoading}>{#if appsLoading}<span class="spin"><Icon name="loader" size={14} /></span>{:else}<Icon name="refresh" size={14} />{/if} {t("settings.refresh")}</button>
    </div>
    <p class="hint">{t("settings.appsHint")}</p>
    {#if app.apps}
      <div class="apps">
        {#each app.apps as a (a.id)}
          <div class="appcard" class:me={a.id === "transcribe"}>
            <div class="apphead">
              <strong>{a.name}</strong>
              {#if a.id === "transcribe"}
                <span class="pill success">{t("settings.thisApp", { version: app.sys?.version ?? "" })}</span>
              {:else if a.installed}
                <span class="pill success"><Icon name="check" size={11} stroke={3} /> {t("settings.installed")}</span>
              {:else}
                <span class="pill">{t("settings.notInstalled")}</span>
              {/if}
              {#if a.latestVersion}<span class="hint">{t("settings.latest", { version: a.latestVersion })}</span>{/if}
            </div>
            <p class="hint">{a.description}</p>
            <div class="row-actions">
              {#if a.id !== "transcribe"}
                {#if a.installed}
                  <button class="btn sm" onclick={() => launch(a.id)} title={a.path ?? ""}><Icon name="external" size={13} /> {t("settings.open")}</button>
                {:else}
                  <button class="btn sm primary" onclick={() => openUrl(a.downloadUrl)}><Icon name="download" size={13} /> {t("settings.download")}</button>
                {/if}
              {/if}
              <button class="btn sm ghost" onclick={() => openUrl(`https://github.com/ellaguno/${REPOS[a.id]}`)}><Icon name="globe" size={13} /> {t("settings.code")}</button>
            </div>
          </div>
        {/each}
      </div>
      {#if app.davMounts.length}
        <p class="hint"><Icon name="drive" size={13} /> {t("settings.davDrives", { list: app.davMounts.map((m) => `${m.name} → ${m.mountPoint}${m.mounted ? "" : t("settings.notMounted")}`).join(" · ") })}</p>
      {/if}
    {:else}
      <p class="hint"><span class="spin"><Icon name="loader" size={13} /></span> {t("settings.searchingApps")}</p>
    {/if}
  </section>

  <section class="card">
    <h2><Icon name="sun" size={17} /> {t("settings.appearance")}</h2>
    <div class="seg">
      {#each [["system", "settings.themeSystem", "monitor"], ["light", "settings.themeLight", "sun"], ["dark", "settings.themeDark", "moon"]] as [id, label, icon]}
        <button class:active={s.theme === id} onclick={() => saveSettings({ theme: id as "system" | "light" | "dark" })}><Icon name={icon} size={15} /> {t(label as Key)}</button>
      {/each}
    </div>
    <div class="switchrow">
      <div>
        <span class="label">{t("settings.checkUpdates")}</span>
        <p class="hint">{t("settings.checkUpdatesHint", { version: app.sys?.version ?? "", target: app.sys?.updateTarget ?? "" })}</p>
      </div>
      <button class="switch" class:on={s.checkUpdates} aria-label={t("settings.checkUpdates")} onclick={() => saveSettings({ checkUpdates: !s.checkUpdates })}></button>
    </div>
    <div><button class="btn sm" onclick={() => import("../lib/updater").then((m) => m.checkForUpdates(false))}><Icon name="refresh" size={14} /> {t("settings.checkNow")}</button></div>
    <p class="hint">{t("settings.modelsAt")} <code>{app.sys?.modelsDir}</code><br />{t("settings.settingsAt")} <code>{app.sys?.settingsPath}</code>{#if app.sys?.logPath}<br />{t("settings.logAt")} <code>{app.sys.logPath}</code>{/if}</p>
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
  .vocab { min-height: 64px; }
  .corr { display: grid; grid-template-columns: 1fr 1fr auto; gap: 6px 8px; align-items: center; }
</style>
