# README media

Screenshots and the hero GIF in `docs/media/` come from the real Svelte interface,
loaded by Vite in headless Chromium with the Tauri backend mocked (`mock.ts`, fictional
data only). Nothing here is part of the app build.

```bash
npm i --no-save puppeteer-core                       # once; not added to package.json
npx vite --port 5301 --strictPort &                  # dev server (stop it afterwards)
node scripts/readme-media/capture.mjs en && node scripts/readme-media/capture.mjs es
scripts/readme-media/build.sh                        # → docs/media/*.png, hero-*.gif (needs ffmpeg, ImageMagick, pngquant)
```

- `capture.mjs <en|es> [all|shots|hero]` writes raw captures to `out/<lang>/` (ignored by git).
  `PORT` and `CHROME` env variables override the dev-server port and the Chromium path
  (default: `~/.cache/ms-playwright/chromium-1234/chrome-linux64/chrome`).
- `app.html?lang=es&theme=dark` opens the mocked app by hand at `http://localhost:5301/scripts/readme-media/app.html`.
- The script prints `MISSING MOCK COMMANDS` if the UI called a command `mock.ts` doesn't answer;
  add it to the `switch` in `mock.ts`.
