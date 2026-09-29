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

---

# Mejores prácticas — cada 2 días

Artículos para despachos de **TI, PMO, consultoría, fiscal y derecho**, en ese orden de rotación.
El plan de temas está en `mejores_practicas.json` (sector, slug, título, si menciona Iurefficient y
qué funciones mostrar). Para añadir temas, agrégalos al final de su sector en ese archivo.

1. **¿Toca?** `python3 iurecms.py practicas`. Si `toca` es `false`, no publicar. Si es `true`,
   `siguiente` trae el tema, el sector (`nombre_sector`), la `cabecera` y la `categoria`.
   Si `pendientes` es menor que 5, decirlo en el resumen para que se añadan temas.

2. **Investigar antes de escribir.** Las prácticas deben ser correctas y actuales. Para fiscal y
   derecho (México), verificar con fuentes oficiales (SAT, DOF, leyes vigentes, Poder Judicial) con
   búsqueda web. Citar artículos de ley o cifras sólo si se verificaron hoy; si no, hablar en general.
   Nada de asesoría para un caso concreto. En fiscal y derecho, cerrar con una línea en cursiva:
   *Este artículo es informativo y no sustituye la asesoría profesional sobre un caso concreto.*

3. **Escribir** (700–1,100 palabras): una entrada que plantee el problema en dos o tres frases,
   luego 5–7 prácticas numeradas como `<h2>` con ejemplos concretos del día a día de ese tipo de
   despacho, y listas cortas donde ayuden. Tono directo, sin relleno ni promesas de marketing.
   - Si `menciona_iurefficient` es `true`: al final, una sección `<h2>Cómo se ve en Iurefficient</h2>`
     con 1–3 pantallas que muestren exactamente lo explicado. Afirmar sólo lo que se ve en la pantalla
     o está documentado en `docs/BLOG_EVOLUCION_VERSIONES.md` o en los manuales del repo; no
     inventar funciones.
   - Si es `false`: **no mencionar Iurefficient en absoluto** (ni en el texto ni en las imágenes).
   - Título: el del plan (se puede pulir). Slug: el del plan, sin cambiarlo.

4. **Pantallas** (sólo si menciona Iurefficient), de `ellaguno/expert-collaborator`:
   `docs/images/plugins/` (tickets `tk_*`, inventario `inv_*`, fiscal `tax_*`, CRM `crm_*`, firma
   `es_*`, facturación `bil_*`, portal `po_*`…), `docs/images/manual/` (proyecto, riesgos, cambios,
   Sábana, presupuesto, portafolio, CRM…), `instance/frontend/public/help-images/` y
   `e2e-tests/recordings/screenshots/`. **Mirar cada imagen antes de usarla.** Recortar con Pillow
   (las capturas son de 2880 px de ancho) para quitar la barra superior con el avatar, nombres de
   prueba («ZZ Prueba…», «test»), valores rotos («None None») y datos que parezcan reales. Si ninguna
   pantalla muestra lo explicado, publicar sin pantalla antes que con una que no corresponde.
   En el cuerpo, `<figure><img src="IMG1" alt="…"></figure>` y en el JSON
   `"images": {"IMG1": "recorte1.png"}`: `publish` las sube y cambia el marcador en ES y EN.

5. **Imagen destacada**: `python3 iurecms.py hero --kicker "Mejores prácticas" --version "<nombre_sector>"
   --caption "<título corto, ≤ 40 caracteres>" --motif <motivo> --out hero.png`. Revisarla.

6. **Publicar** con `publish`, añadiendo al JSON `"category": "<categoria>"` y
   `"brand": "<cabecera>"`, más la versión en inglés en `en` (con los nombres de la interfaz en
   inglés de `instance/frontend/messages/en.json`). Verificar ES y EN como en las novedades.

---

# LinkedIn — un post por cada artículo de mejores prácticas

Sólo para los artículos de **mejores prácticas** (nunca los de versiones), el mismo día que se
publican. Se publica en el perfil personal de Eduardo con `linkedin.py` (token en
`LINKEDIN_ACCESS_TOKEN`, permisos `openid`, `profile` y `w_member_social`; caduca cada 60 días).

1. **Escribir el post** siguiendo `VOZ_LINKEDIN.md` al pie de la letra (léelo completo cada vez).
   Guardarlo como JSON: `{"text": …, "url": <URL del artículo en español>, "title": <título del
   artículo>, "description": <una frase>, "image_url": <URL de la imagen destacada publicada>}`.
2. **Revisar en seco**: `python3 linkedin.py post post.json --dry-run`.
3. **Modo revisión (hasta el 13 de octubre de 2026 inclusive): NO publicar.** Poner el texto
   completo del post en el resumen final y pedir a Eduardo que responda «publícalo» (o sus
   cambios) en esta misma sesión. Sólo cuando lo pida: aplicar los cambios y publicar con
   `python3 linkedin.py post post.json`, una sola vez.
4. **Modo directo (desde el 14 de octubre de 2026)**: publicar con `linkedin.py post` y poner en el
   resumen el texto y el enlace del post.
5. Si LinkedIn responde 401, el token caducó: no reintentar y avisar en el resumen que hay que
   renovarlo (Token generator de LinkedIn Developers, app Iurefficient).
6. Nunca publicar dos veces el mismo post: si la respuesta fue 201 o trae un `urn`, ya está.
