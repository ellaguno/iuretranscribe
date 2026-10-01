// Drives the real IureTranscribe UI (Vite dev server + mock.ts backend) and saves
// README screenshots and the frames of the hero GIF.
//   node scripts/readme-media/capture.mjs [en|es] [all|shots|hero]
// Needs the dev server: npx vite --port 5301 --strictPort  (PORT env to change it)
// Output: scripts/readme-media/out/<lang>/*.png and out/<lang>/hero/ (frames + frames.txt)
import puppeteer from 'puppeteer-core';
import fs from 'node:fs';
import os from 'node:os';

const LANG = process.argv[2] === 'es' ? 'es' : 'en';
const WHAT = process.argv[3] ?? 'all';
const PORT = process.env.PORT ?? '5301';
const CHROME = process.env.CHROME ?? `${os.homedir()}/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome`;
const OUT = new URL(`./out/${LANG}/`, import.meta.url).pathname;
fs.mkdirSync(OUT, { recursive: true });
const L = (en, es) => (LANG === 'es' ? es : en);
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

const browser = await puppeteer.launch({ executablePath: CHROME, args: ['--no-sandbox', '--font-render-hinting=none', `--lang=${LANG}`] });
const missing = new Set();

async function open({ theme = 'light', jobs = '' } = {}) {
  const p = await browser.newPage();
  p.on('pageerror', (e) => console.log('ERR:', e.message));
  p.on('console', (m) => {
    const t = m.text();
    if (t.startsWith('mock: sin respuesta')) missing.add(t);
  });
  await p.setViewport({ width: 1280, height: 800, deviceScaleFactor: 1.5 });
  await p.goto(`http://localhost:${PORT}/scripts/readme-media/app.html?lang=${LANG}&theme=${theme}${jobs ? `&jobs=${jobs}` : ''}`, { waitUntil: 'networkidle0' });
  await wait(1200);
  await p.addStyleTag({ content: '*,*::before,*::after{caret-color:transparent!important;transition:none!important;animation:none!important} .toasts,.toast{display:none!important} ::-webkit-scrollbar{display:none}' });
  await p.evaluate(() => new MutationObserver(() => document.querySelectorAll('input,textarea').forEach((e) => (e.spellcheck = false))).observe(document.body, { childList: true, subtree: true }));
  return p;
}
const nav = async (p, target) => { await p.evaluate((t) => window.__emit('launch-args', [t]), target); await wait(500); };
const foldMeta = async (p) => { await p.evaluate(() => { const f = document.querySelector('.meta-toggle[aria-expanded="true"]'); if (f) f.click(); }); await wait(300); };
/** Scrolls the document pane so the rendered Markdown starts near the top, keeping the "Saved in" row. */
const toDoc = (p) => p.evaluate(() => {
  const b = [...document.querySelectorAll('.body.scroll')].pop();
  const h = b.querySelector('h1, h2');
  const row = [...b.querySelectorAll('*')].find((e) => e.children.length === 0 && /^(Saved in|Guardad[oa] en)/.test(e.textContent.trim()));
  const target = row ?? h;
  b.scrollTop += target.getBoundingClientRect().top - b.getBoundingClientRect().top - 16;
});
const tab = async (p, i) => { await p.evaluate((i) => document.querySelectorAll('.tabs > button:not(.btn)')[i].click(), i); await wait(350); };
/** Scrolls the settings card whose h2 contains `text` to the top of the scroll area. */
const scrollToCard = (p, text, offset = 12) => p.evaluate((text, offset) => {
  const h = [...document.querySelectorAll('section.card h2')].find((e) => e.textContent.includes(text));
  const box = h.closest('.scroll');
  const card = h.closest('section');
  box.scrollTop += card.getBoundingClientRect().top - box.getBoundingClientRect().top - offset;
}, text, offset);

// ------------------------------------------------------------------ screenshots
async function shots() {
  // 1) File queue: a file transcribing with live segments, another queued.
  {
    const p = await open();
    const dir = L('/home/demo/Documents/Meetings', '/home/demo/Documentos/Reuniones');
    const files = L(['Board meeting - Q3 results.mp4', 'Witness interview.wav'], ['Junta de consejo - resultados T3.mp4', 'Entrevista a testigo.wav']);
    await p.evaluate((paths) => window.__emit('launch-args', paths), files.map((f) => `${dir}/${f}`));
    await wait(700);
    await foldMeta(p);
    const lines = L(
      [
        'Good afternoon, everyone. Let\'s start with the third quarter results.',
        'Revenue grew eleven percent compared with the same period last year.',
        'Most of the growth came from the new logistics contracts in the north.',
        'Operating costs rose four percent, mainly fuel and maintenance.',
        'The audit committee recommends keeping the dividend unchanged.',
        'Next item: the renewal of the credit line with the bank.',
        'Legal confirmed there are no pending claims that affect the renewal.',
        'The new term would be three years at a slightly lower rate.',
      ],
      [
        'Buenas tardes a todos. Empezamos con los resultados del tercer trimestre.',
        'Los ingresos crecieron once por ciento respecto al mismo periodo del año pasado.',
        'La mayor parte del crecimiento vino de los nuevos contratos de logística en el norte.',
        'Los costos de operación subieron cuatro por ciento, sobre todo combustible y mantenimiento.',
        'El comité de auditoría recomienda mantener el dividendo sin cambios.',
        'Siguiente punto: la renovación de la línea de crédito con el banco.',
        'Jurídico confirmó que no hay litigios pendientes que afecten la renovación.',
        'El nuevo plazo sería de tres años con una tasa ligeramente menor.',
      ],
    );
    let t = 0;
    for (const line of lines) {
      const len = 6000 + line.length * 45;
      await p.evaluate((a, b, s) => window.__seg(a, b, s), t, t + len, line);
      t += len + 400;
    }
    await p.evaluate(() => window.__progress(38));
    await wait(600);
    await p.screenshot({ path: OUT + 'queue.png' });
    await p.close();
  }
  // 2) Minutes of the stored meeting.
  {
    const p = await open();
    await tab(p, 2);
    await toDoc(p);
    await wait(200);
    await p.screenshot({ path: OUT + 'minutes.png' });
    // Dark-theme shot: same meeting, transcript with who spoke.
    await p.close();
    const d = await open({ theme: 'dark' });
    await tab(d, 1);
    await toDoc(d);
    await wait(200);
    await d.screenshot({ path: OUT + 'summary-dark.png' });
    await d.close();
  }
  // 3) Settings → Vocabulary and corrections, 4) Settings → AI assistants.
  {
    const p = await open();
    await nav(p, 'iuretranscribe://settings');
    await wait(400);
    await scrollToCard(p, L('Vocabulary and corrections', 'Vocabulario y correcciones'));
    await wait(200);
    await p.screenshot({ path: OUT + 'vocabulary.png' });
    await scrollToCard(p, L('AI assistants', 'Asistentes de IA'));
    await wait(200);
    await p.screenshot({ path: OUT + 'assistants.png' });
    await p.close();
  }
}

// ------------------------------------------------------------------ hero GIF frames
async function hero() {
  const HD = OUT + 'hero/';
  fs.rmSync(HD, { recursive: true, force: true });
  fs.mkdirSync(HD, { recursive: true });
  const p = await open();
  const frames = [];
  const shot = async (secs) => {
    await wait(80);
    const file = `${String(frames.length).padStart(3, '0')}.png`;
    await p.screenshot({ path: HD + file });
    frames.push([file, secs]);
  };
  const setRec = (o) => p.evaluate((o) => Object.assign(window.__rec, o), o);

  await nav(p, 'iuretranscribe://record');
  await p.click('input[id^="m-date"]');
  await p.keyboard.type(L('October 1, 2026, 10:00', '1 de octubre de 2026, 10:00'));
  await p.click('textarea[id^="m-people"]');
  await p.keyboard.type('Ana Torres, Carlos Ruiz, Laura Méndez');
  await p.click('input[id^="m-place"]');
  await p.keyboard.type(L('Video call', 'Videollamada'));
  await p.evaluate(() => document.activeElement.blur());
  await shot(1.2);
  await p.click('.rec-btn');
  await wait(500);
  const [me, other] = await p.evaluate(() => window.__speakers());
  await setRec({ elapsedSecs: 1, micLevel: 0.1, sysLevel: 0.05 });
  await wait(350);
  await shot(0.5);

  const dialog = L(
    [
      [me, 'Good morning, Carlos. Let\'s go over the supplier claim before Friday.'],
      [other, 'Sure. They delivered the second shipment three weeks late.'],
      [me, 'The contract allows a two percent penalty per week of delay.'],
      [other, 'So we can deduct six percent from the last invoice?'],
      [me, 'Yes, and I\'ll send them a formal notice today.'],
      [other, 'Perfect. Copy Laura so finance can hold the payment.'],
    ],
    [
      [me, 'Buenos días, Carlos. Revisemos el reclamo al proveedor antes del viernes.'],
      [other, 'Claro. Entregaron el segundo embarque con tres semanas de retraso.'],
      [me, 'El contrato permite una pena del dos por ciento por semana de atraso.'],
      [other, '¿Entonces podemos descontar el seis por ciento de la última factura?'],
      [me, 'Sí, y hoy mismo les envío la notificación formal.'],
      [other, 'Perfecto. Copia a Laura para que finanzas retenga el pago.'],
    ],
  );
  let secs = 1;
  let ms = 600;
  for (const [spk, text] of dialog) {
    const mine = spk === me;
    for (let k = 0; k < 3; k++) {
      secs += 2;
      await setRec({ elapsedSecs: secs, micLevel: mine ? 0.45 + 0.35 * Math.random() : 0.04 + 0.04 * Math.random(), sysLevel: mine ? 0.03 + 0.03 * Math.random() : 0.4 + 0.35 * Math.random() });
      await wait(300);
      await shot(0.2);
    }
    const len = 3500 + text.length * 45;
    await p.evaluate((a, z, t, s) => window.__live(a, z, t, s), ms, ms + len, text, spk);
    ms += len + 300;
    secs += 1;
    await setRec({ elapsedSecs: secs, micLevel: mine ? 0.2 : 0.05, sysLevel: mine ? 0.04 : 0.2 });
    await wait(300);
    await shot(0.75);
  }
  // Stop → the recording becomes a finished job with the transcript (who spoke).
  await p.click('.rec-btn');
  await wait(1200);
  await foldMeta(p);
  await shot(2.2);
  await p.close();

  // ffconcat list (the last frame is repeated so its duration is honoured).
  const list = ['ffconcat version 1.0', ...frames.flatMap(([f, d]) => [`file '${f}'`, `duration ${d}`]), `file '${frames.at(-1)[0]}'`];
  fs.writeFileSync(HD + 'frames.txt', list.join('\n') + '\n');
  console.log(`hero: ${frames.length} frames, ${frames.reduce((a, [, d]) => a + d, 0).toFixed(1)} s`);
}

if (WHAT === 'all' || WHAT === 'shots') await shots();
if (WHAT === 'all' || WHAT === 'hero') await hero();
await browser.close();
if (missing.size) console.log('MISSING MOCK COMMANDS:\n' + [...missing].join('\n'));
else console.log('mock: all commands answered');
