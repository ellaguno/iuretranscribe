# Novedades de versión en iurefficient.com — procedimiento diario

Cada día se revisa si Iurefficient (`ellaguno/expert-collaborator`) tiene versiones sin
artículo en https://iurefficient.com/articulos/ y se publica **como máximo un artículo por día**,
el de la versión menor más antigua pendiente. Así el rezago se va poniendo al día solo y,
una vez al día, sólo aparecen versiones nuevas.

## Requisitos

- Variable de entorno `IUREF_CMS_TOKEN`: token de la API de cms_simple (≥ 1.37; renombrar con
  `update` pide ≥ 1.37.1), creado en **Usuarios → Acceso por API** del admin con acceso a
  «Artículos». `iurecms.py` usa entonces `/admin/api/` (listado, lectura, subida y guardado en
  JSON) en lugar de los formularios.
- Respaldo, sólo si no hay token: `IUREF_CMS_USER` y `IUREF_CMS_PASS` (cuenta del admin); el
  script inicia sesión e imita al navegador, como antes.
- `pip install pillow` para generar la imagen.
- Clon de `ellaguno/expert-collaborator` (basta `--depth 1`; las etiquetas se leen del remoto).

## Pasos

1. **¿Qué sigue?**

   ```
   python3 iurecms.py status --repo /ruta/a/expert-collaborator
   ```

   Devuelve `next: null` si no hay nada pendiente → terminar sin publicar.
   Si no, `next.label` (p. ej. `v4.86`) y `next.versions` (p. ej. `v4.86.0`, `v4.86.1`).
   Cuentan como cubiertas las versiones que aparecen en los títulos y cuerpos de los artículos
   «¿Qué hay de nuevo?», incluidos borradores.

2. **Fuente del texto.** En el repo, `docs/BLOG_EVOLUCION_VERSIONES.md` tiene una sección
   `## vX.Y.Z — …` por versión, ya escrita para el público. Tomar las secciones de
   `next.versions`. Si alguna falta, redactarla a partir del mensaje del commit de release
   (`git log`), las notas de la etiqueta y los `docs/` relacionados, sin inventar cifras.

3. **Redactar el artículo** (español, tono del blog: concreto, sin marketing vacío):
   - `title`: `¿Qué hay de nuevo? vX.Y — <frase corta de lo principal>`
     (si sólo son parches de una menor ya publicada, usar la versión completa: `vX.Y.Z`).
   - `slug`: `que-hay-de-nuevo-vX-Y-<frase-corta>` en minúsculas, sin acentos.
   - `excerpt`: 1–3 frases que enganchen (se muestra como entrada y en el listado).
   - `seo_desc`: ≤ 160 caracteres.
   - `tags`: 3–5 temas + `version`.
   - `body_html`: un `<h2>vX.Y.Z — título</h2>` por versión (de la más nueva a la más
     vieja, como el blog), párrafos `<p>`, `<strong>`, `<em>`, `<code>`, `<h3>` para
     subtemas, `<hr>` entre versiones. HTML limpio, sin estilos en línea. Omitir detalles
     internos (nombres de servidores, clientes, rutas de código, secretos).
   - `date`: fecha de hoy (AAAA-MM-DD). `image_file`: la imagen del paso 4.
   - `en`: **versión en inglés, obligatoria** (`publish` se niega sin ella). Objeto con
     `title` (`What’s new? vX.Y — …`), `excerpt`, `seo_desc`, `tags` (con `release` en vez de
     `version`) y `body_html`. Traducción fiel y natural del español, misma estructura HTML.
     El slug es el mismo en ambos idiomas; la versión inglesa queda en `/en/articulos/<slug>`.

4. **Imagen destacada** (1600×860, colores de la marca):

   ```
   python3 iurecms.py hero --version vX.Y --caption "<frase de ≤ 40 caracteres>" \
       --motif <motivo> --out vX-Y.png
   ```

   Motivos: `documento`, `chat`, `grafica`, `red`, `escudo`, `calendario`, `voz`,
   `carpeta`, o `abstracto` si ninguno es alusivo. Revisar la imagen antes de publicar.

5. **Publicar**: `python3 iurecms.py publish articulo.json`. Se niega a duplicar un slug.
   Comprobar después que cargan (HTTP 200) la URL devuelta y `/en/articulos/<slug>`, y que
   el artículo sale en https://iurefficient.com/articulos/. Para comprobar usar `curl` con su
   User-Agent por omisión: el servidor responde 406 a algunos User-Agent de navegador.

6. **Corregir un artículo ya publicado**: `python3 iurecms.py update <slug-actual> cambios.json`,
   con sólo las claves a cambiar (mismo formato que `publish`, incluido `en`). Si trae `slug`,
   el CMS renombra el artículo.

## Notas

- Categoría «Novedades de versión», autor `Iurefficient`, cabecera `derecho`, estado
  `published`: son los valores por omisión de `publish`.
- El servidor rechaza con 403 el User-Agent por omisión de Python; `iurecms.py` ya envía uno propio.
- Por la API, el cuerpo va blindado como lo hace el panel (`=?rb64?=`, base64 invertido) para que
  el firewall del hosting no lo corte por llevar HTML. La API crea o actualiza según exista el slug;
  `publish` comprueba antes que no exista y se detiene si la respuesta dice que no se creó.
