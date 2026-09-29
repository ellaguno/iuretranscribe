# Novedades de versión en iurefficient.com — procedimiento diario

Cada día se revisa si Iurefficient (`ellaguno/expert-collaborator`) tiene versiones sin
artículo en https://iurefficient.com/articulos/ y se publica **como máximo un artículo por día**,
el de la versión menor más antigua pendiente. Así el rezago se va poniendo al día solo y,
una vez al día, sólo aparecen versiones nuevas.

## Requisitos

- Variables de entorno `IUREF_CMS_USER` y `IUREF_CMS_PASS` (cuenta del admin de cms_simple).
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

4. **Imagen destacada** (1600×860, colores de la marca):

   ```
   python3 iurecms.py hero --version vX.Y --caption "<frase de ≤ 40 caracteres>" \
       --motif <motivo> --out vX-Y.png
   ```

   Motivos: `documento`, `chat`, `grafica`, `red`, `escudo`, `calendario`, `voz`,
   `carpeta`, o `abstracto` si ninguno es alusivo. Revisar la imagen antes de publicar.

5. **Publicar**: `python3 iurecms.py publish articulo.json`. Se niega a duplicar un slug.
   Comprobar después que la URL devuelta carga (HTTP 200) y que el artículo sale en
   https://iurefficient.com/articulos/.

## Notas

- Categoría `novedades-de-version`, autor `Iurefficient`, cabecera `derecho`, estado
  `published`: son los valores por omisión de `publish`.
- El servidor rechaza con 403 el User-Agent por omisión de Python; `iurecms.py` ya envía uno propio.
