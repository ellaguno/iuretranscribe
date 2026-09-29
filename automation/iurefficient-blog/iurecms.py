#!/usr/bin/env python3
"""Publica los artículos «¿Qué hay de nuevo?» de Iurefficient en su cms_simple.

Subcomandos:
  status  --repo RUTA          Qué versiones ya tienen artículo y cuál sigue.
  hero    --version V --out F  Genera la imagen destacada (abstracta, colores de marca).
  publish ARTICULO.json        Sube la imagen y crea el artículo (español e inglés).
  update  SLUG CAMBIOS.json    Cambia campos de un artículo existente (también el slug).
  practicas                    ¿Toca artículo de mejores prácticas? Y cuál (según mejores_practicas.json).

Credenciales: IUREF_CMS_TOKEN, token de la API de cms_simple (≥ 1.37; renombrar con `update` pide
≥ 1.37.1), creado en Usuarios → Acceso por API con acceso a «articulos». Sin token se usa el método
anterior (login con IUREF_CMS_USER / IUREF_CMS_PASS y formularios del admin), sólo como respaldo.
Requiere Pillow sólo para `hero`.
"""
import argparse
import base64
import html
import http.client
import http.cookiejar
import json
import math
import mimetypes
import os
import random
import re
import subprocess
import sys
import time
import urllib.parse
import urllib.error
import urllib.request
import unicodedata
import uuid
from html.parser import HTMLParser

BASE = os.environ.get("IUREF_CMS_BASE", "https://iurefficient.com")
TYPE = "articulos"
VERSION_TITLE = re.compile(r"\bv(\d+)\.(\d+)\b")
VERSION_FULL = re.compile(r"\bv?(\d+)\.(\d+)\.(\d+)\b")


UA = "iurecms/1.1 (+publicador de novedades)"  # el servidor responde 403 al User-Agent por omisión de urllib


class ApiCms:
    """cms_simple ≥ 1.37: /admin/api/ con token. Mismo contrato que FormCms."""

    def __init__(self, token):
        self.token = token

    def call(self, method, path, body=None, ctype=None, tries=4):
        headers = {"User-Agent": UA, "X-CMS-Token": self.token, "Accept": "application/json"}
        if ctype:
            headers["Content-Type"] = ctype
        for i in range(tries):
            req = urllib.request.Request(BASE + "/admin/api/" + path, data=body, method=method, headers=headers)
            try:
                with urllib.request.urlopen(req) as r:
                    return r.status, json.loads(r.read().decode("utf-8"))
            except urllib.error.HTTPError as e:
                raw = e.read().decode("utf-8", "replace")
                try:
                    return e.code, json.loads(raw)
                except ValueError:
                    raise SystemExit(f"La API respondió {e.code} sin JSON ({path}): {raw[:300]}")
            except (http.client.IncompleteRead, ConnectionError, urllib.error.URLError):
                # sólo se reintentan lecturas; repetir un POST podría guardar dos veces
                if i == tries - 1 or method != "GET":
                    raise
                time.sleep(2 ** i)

    def login(self):
        code, j = self.call("GET", "")
        if code != 200 or not j.get("ok"):
            raise SystemExit(f"Token rechazado por el CMS: {j.get('error', code)}")
        if "*" not in j.get("types", []) and TYPE not in j.get("types", []):
            raise SystemExit(f"El token no tiene acceso a «{TYPE}»")

    def articles(self):
        """[(slug, title, status)] de todos los artículos (incluye borradores)."""
        out, page = [], 1
        while True:
            code, j = self.call("GET", f"items/{TYPE}?per=200&page={page}")
            if code != 200:
                raise SystemExit(f"No pude leer el listado: {j.get('error', code)}")
            out += [(it["slug"], it["title"], it["status"]) for it in j["items"]]
            if page >= j["pages"]:
                return out
            page += 1

    def get_item(self, slug):
        code, j = self.call("GET", f"items/{TYPE}/{urllib.parse.quote(slug)}")
        return j["item"] if code == 200 else None

    def body_of(self, slug):
        b = (self.get_item(slug) or {}).get("body", "")
        return b.get("es", "") if isinstance(b, dict) else str(b)

    def exists(self, slug):
        return self.get_item(slug) is not None

    def date_of(self, slug):
        return str((self.get_item(slug) or {}).get("date", ""))

    def upload(self, path):
        boundary = uuid.uuid4().hex
        ctype = mimetypes.guess_type(path)[0] or "application/octet-stream"
        with open(path, "rb") as f:
            data = f.read()
        body = (f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{os.path.basename(path)}"\r\n'
                f"Content-Type: {ctype}\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
        code, j = self.call("POST", "upload", body, f"multipart/form-data; boundary={boundary}")
        if not j.get("path"):
            raise SystemExit(f"La subida de imagen falló: {j.get('error', code)}")
        return j["path"]

    def save_article(self, a, slug=None):
        """Crea (slug=None) o modifica el artículo `slug`; sólo se tocan las claves presentes en `a`."""
        item = api_item(a)
        if slug:
            if not self.exists(slug):
                raise SystemExit(f"No encontré el artículo {slug}")
            item["slug"] = slug
            if a.get("slug") and a["slug"] != slug:
                item["new_slug"] = a["slug"]
        else:
            item["slug"] = a["slug"]
        # Blindaje del panel (=?rb64?=, base64 invertido): el firewall del hosting no ve HTML en el cuerpo.
        raw = json.dumps(item, ensure_ascii=False).encode("utf-8")
        body = ("=?rb64?=" + base64.b64encode(raw).decode("ascii")[::-1]).encode("ascii")
        code, j = self.call("POST", f"items/{TYPE}", body, "text/plain; charset=utf-8")
        if not j.get("ok"):
            raise SystemExit(f"El CMS no guardó el artículo: {j.get('errors') or j.get('error') or code}")
        if not slug and j.get("created") is False:
            raise SystemExit(f"El slug {a['slug']} ya existía y se actualizó en vez de crearse; revísalo")
        if item.get("new_slug") and j.get("renamed_from") != slug:
            raise SystemExit(f"Se guardaron los cambios pero el CMS no renombró {slug} (renombrar por la API pide cms_simple ≥ 1.37.1)")
        return j["item"]["slug"]


class FormCms:
    """Respaldo para cms_simple < 1.37 o sin token: imita al navegador en el admin."""

    def __init__(self):
        self.jar = http.cookiejar.CookieJar()
        self.op = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))
        self.op.addheaders = [("User-Agent", UA)]
        self.csrf = None

    def get(self, path, tries=4):
        for i in range(tries):
            try:
                with self.op.open(BASE + path) as r:
                    return r.geturl(), r.read().decode("utf-8")
            except (http.client.IncompleteRead, ConnectionError, urllib.error.URLError):
                if i == tries - 1:
                    raise
                time.sleep(2 ** i)

    def post(self, path, data=None, body=None, ctype=None):
        if body is None:
            body = urllib.parse.urlencode(data, doseq=True).encode()
            ctype = "application/x-www-form-urlencoded"
        req = urllib.request.Request(BASE + path, data=body, headers={"Content-Type": ctype})
        with self.op.open(req) as r:
            return r.geturl(), r.read().decode("utf-8")

    def _csrf_from(self, page):
        m = re.search(r'name="_csrf" value="([^"]+)"', page)
        if not m:
            raise SystemExit("No encontré el token CSRF en la página del admin")
        self.csrf = m.group(1)

    def login(self):
        user, pw = os.environ.get("IUREF_CMS_USER"), os.environ.get("IUREF_CMS_PASS")
        if not user or not pw:
            raise SystemExit("Faltan IUREF_CMS_USER / IUREF_CMS_PASS en el entorno")
        _, page = self.get("/admin/?p=login")
        self._csrf_from(page)
        url, page = self.post("/admin/?p=login", {"_csrf": self.csrf, "user": user, "pass": pw})
        if "p=login" in url:
            raise SystemExit("Login rechazado por el CMS")
        self._csrf_from(page)

    def articles(self):
        """[(slug, title, status)] de todas las páginas del listado."""
        out, page_no = [], 1
        while True:
            _, page = self.get(f"/admin/?p=content&type={TYPE}&page={page_no}")
            rows = re.findall(
                r'<tr>\s*<td><a href="/admin/\?p=edit&type=articulos&slug=([^"]+)"><strong>(.*?)</strong>.*?'
                r'<span class="ad-pill[^"]*">([^<]+)</span>', page, re.S)
            new = [(s, html.unescape(t), st.strip().lower()) for s, t, st in rows if s not in {r[0] for r in out}]
            if not new:
                return out
            out += new
            page_no += 1

    def body_of(self, slug):
        _, page = self.get(f"/admin/?p=edit&type={TYPE}&slug={urllib.parse.quote(slug)}")
        m = re.search(r'<textarea name="body\[es\]"[^>]*>(.*?)</textarea>', page, re.S)
        return html.unescape(m.group(1)) if m else ""

    def upload(self, path):
        boundary = uuid.uuid4().hex
        ctype = mimetypes.guess_type(path)[0] or "application/octet-stream"
        with open(path, "rb") as f:
            data = f.read()
        parts = [
            f'--{boundary}\r\nContent-Disposition: form-data; name="_csrf"\r\n\r\n{self.csrf}\r\n'.encode(),
            f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{os.path.basename(path)}"\r\n'
            f"Content-Type: {ctype}\r\n\r\n".encode() + data + b"\r\n",
            f"--{boundary}--\r\n".encode(),
        ]
        _, resp = self.post("/admin/?p=upload", body=b"".join(parts), ctype=f"multipart/form-data; boundary={boundary}")
        j = json.loads(resp)
        if not j.get("path"):
            raise SystemExit(f"La subida de imagen falló: {resp[:300]}")
        return j["path"]

    def exists(self, slug):
        return slug in {s for s, _, _ in self.articles()}

    def date_of(self, slug):
        _, page = self.get(f"/admin/?p=edit&type={TYPE}&slug={urllib.parse.quote(slug)}")
        return form_values(page).get("date", "")

    def save_article(self, a, slug=None):
        return self.save(article_fields(a), slug=slug)

    def save(self, changes, slug=None):
        """Guarda un artículo: parte de los valores actuales del formulario (vacíos si es nuevo)
        y aplica `changes`. Con `slug` edita ese artículo; si changes["slug"] difiere, el CMS lo renombra."""
        path = f"/admin/?p=edit&type={TYPE}" + (f"&slug={urllib.parse.quote(slug)}" if slug else "")
        _, page = self.get(path)
        fields = form_values(page)
        if slug and fields.get("slug") != slug:
            raise SystemExit(f"No encontré el artículo {slug}")
        # Campos de los formularios del pack de audio, anidados dentro del editor: no son del artículo.
        for k in ("action", "type", "lang", "back"):
            fields.pop(k, None)
        changes = dict(changes)
        if changes.get("category"):
            # El select del editor espera el slug de una categoría existente, o "__new__" y el nombre aparte.
            opts = dict(re.findall(r'<option value="([^"]+)"[^>]*>([^<]*)</option>',
                                   (re.search(r'<select name="category".*?</select>', page, re.S) or re.search("", "")).group(0)))
            name = changes["category"]
            hit = next((v for v, lab in opts.items() if v in (name, slugify(name)) or html.unescape(lab).strip().lower() == name.lower()), None)
            if hit:
                changes["category"] = hit
            else:
                changes["category"], changes["category__new"] = "__new__", name
        fields.update(changes)
        self.csrf = fields["_csrf"]
        url, page = self.post(path, fields)
        m = re.search(r"slug=([^&#]+)", url)
        if not m:
            err = re.findall(r'class="ad-(?:error|alert)[^"]*"[^>]*>(.*?)<', page, re.S)
            raise SystemExit(f"El CMS no guardó el artículo: {err or url}")
        return urllib.parse.unquote(m.group(1))


class _FormParser(HTMLParser):
    """Valores del formulario principal del editor, como los enviaría el navegador."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.depth, self.values, self._ta, self._sel, self._sel_first = 0, {}, None, None, None

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if tag == "form" and "data-slug-source" in a:
            self.depth = 1
            return
        if not self.depth:
            return
        name = a.get("name")
        if tag == "input" and name and a.get("type") not in ("submit", "button", "file"):
            if a.get("type") in ("checkbox", "radio") and "checked" not in a:
                return
            self.values[name] = a.get("value", "")
        elif tag == "textarea" and name:
            self._ta, self.values[name] = name, ""
        elif tag == "select" and name:
            self._sel, self._sel_first = name, None
        elif tag == "option" and self._sel:
            if self._sel_first is None:
                self._sel_first = a.get("value", "")
            if "selected" in a:
                self.values[self._sel] = a.get("value", "")

    def handle_endtag(self, tag):
        if tag == "form" and self.depth:
            self.depth = 0
        elif tag == "textarea":
            self._ta = None
        elif tag == "select" and self._sel:
            self.values.setdefault(self._sel, self._sel_first or "")
            self._sel = None

    def handle_data(self, data):
        if self._ta:
            self.values[self._ta] += data


def form_values(page):
    p = _FormParser()
    p.feed(page)
    if "_csrf" not in p.values:
        raise SystemExit("No encontré el formulario del editor")
    return p.values


def article_fields(a):
    """Campos del CMS a partir del JSON del artículo (claves ausentes = no se tocan)."""
    f = {}
    for lang, src in (("es", a), ("en", a.get("en") or {})):
        for key, field in (("title", "title"), ("excerpt", "excerpt"), ("body_html", "body"),
                           ("seo_title", "seo_title"), ("seo_desc", "seo_desc"), ("tags", "tags")):
            if key in src:
                f[f"{field}[{lang}]"] = src[key]
    for key in ("slug", "date", "status", "author", "category", "image", "brand"):
        if key in a:
            f[key] = a[key]
    return f


def api_item(a):
    """Cuerpo JSON de la API a partir del JSON del artículo (claves ausentes = no se tocan)."""
    item = {}
    for lang, src in (("es", a), ("en", a.get("en") or {})):
        for key, field in (("title", "title"), ("excerpt", "excerpt"), ("body_html", "body"),
                           ("seo_title", "seo_title"), ("seo_desc", "seo_desc"), ("tags", "tags")):
            if key in src:
                item.setdefault(field, {})[lang] = src[key]
    for key in ("date", "status", "author", "category", "image", "brand"):
        if key in a:
            item[key] = a[key]
    return item


def connect():
    tok = os.environ.get("IUREF_CMS_TOKEN", "").strip()
    cms = ApiCms(tok) if tok else FormCms()
    if not tok:
        print("Aviso: sin IUREF_CMS_TOKEN; uso el login del admin (respaldo).", file=sys.stderr)
    cms.login()
    return cms


def slugify(s):
    s = unicodedata.normalize("NFD", s.lower())
    s = "".join(c for c in s if unicodedata.category(c) != "Mn")
    return re.sub(r"[^a-z0-9]+", "-", s).strip("-")


def vkey(s):
    return tuple(int(x) for x in s.split("."))


def repo_versions(repo):
    out = subprocess.run(["git", "-C", repo, "ls-remote", "--tags", "origin"], capture_output=True, text=True, check=True).stdout
    vs = {m.group(1) for m in re.finditer(r"refs/tags/v(\d+\.\d+\.\d+)$", out, re.M)}
    return sorted(vs, key=vkey)


def cmd_status(args):
    cms = connect()
    covered, existing = set(), []
    for slug, title, status in cms.articles():
        m = VERSION_TITLE.search(title)
        if not m or "nuevo" not in title.lower():
            continue
        existing.append({"slug": slug, "title": title, "status": status})
        minor = f"{m.group(1)}.{m.group(2)}"
        body = cms.body_of(slug)
        found = {".".join(x) for x in VERSION_FULL.findall(title + " " + body)}
        covered |= found or {minor + ".0"}
    top = max(covered, key=vkey) if covered else "0.0.0"
    pending = [v for v in repo_versions(args.repo) if vkey(v) > vkey(top)]
    groups = {}
    for v in pending:
        groups.setdefault(".".join(v.split(".")[:2]), []).append(v)
    nxt = None
    if groups:
        minor = min(groups, key=vkey)
        vs = groups[minor]
        # Si el .0 ya tenía artículo, la entrega es sólo de parches: se titula con el parche.
        label = minor if minor + ".0" in vs else vs[-1]
        nxt = {"label": "v" + label, "versions": ["v" + v for v in vs]}
    print(json.dumps({
        "articles": existing,
        "highest_covered": "v" + top,
        "pending_minors": list(groups),
        "next": nxt,
    }, ensure_ascii=False, indent=2))


# ---------------------------------------------------------------- imagen

PALETTE = {
    "bg0": (26, 26, 46), "bg1": (49, 46, 129),
    "p500": (79, 70, 229), "p400": (99, 102, 241), "p300": (129, 140, 248),
    "a500": (6, 182, 212), "a400": (34, 211, 238), "a300": (103, 232, 249),
    "white": (255, 255, 255),
}
MOTIFS = ["abstracto", "documento", "chat", "grafica", "red", "escudo", "calendario", "voz", "carpeta"]


def _font(size, bold=True):
    for p in ([
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ] if bold else [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ]):
        if os.path.exists(p):
            from PIL import ImageFont
            return ImageFont.truetype(p, size)
    from PIL import ImageFont
    return ImageFont.load_default()


def _mix(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def _motif(d, name, cx, cy, s, rnd):
    """Pictograma de líneas en blanco translúcido, centrado en (cx, cy), tamaño s."""
    w = max(4, s // 40)
    c = (255, 255, 255, 215)
    soft = (255, 255, 255, 60)
    if name == "documento":
        for k in (2, 1):
            o = k * s * 0.06
            d.rounded_rectangle([cx - s * .32 + o, cy - s * .42 - o, cx + s * .32 + o, cy + s * .42 - o], s * .04, outline=soft, width=w)
        d.rounded_rectangle([cx - s * .32, cy - s * .42, cx + s * .32, cy + s * .42], s * .04, outline=c, width=w)
        for i in range(6):
            y = cy - s * .26 + i * s * .1
            d.line([cx - s * .2, y, cx + s * (.2 if i % 3 else .05), y], fill=c, width=w)
    elif name == "chat":
        d.rounded_rectangle([cx - s * .45, cy - s * .38, cx + s * .2, cy + s * .02], s * .08, outline=c, width=w)
        d.polygon([(cx - s * .3, cy + s * .02), (cx - s * .34, cy + s * .14), (cx - s * .2, cy + s * .02)], fill=c)
        d.rounded_rectangle([cx - s * .1, cy + s * .08, cx + s * .45, cy + s * .4], s * .08, outline=c, width=w)
        for i in range(3):
            x = cx + s * (.05 + i * .12)
            d.ellipse([x - w, cy + s * .24 - w, x + w, cy + s * .24 + w], fill=c)
    elif name == "grafica":
        hs = [rnd.uniform(.25, .8) for _ in range(5)]
        for i, h in enumerate(hs):
            x = cx - s * .4 + i * s * .18
            d.rounded_rectangle([x, cy + s * .4 - s * h, x + s * .11, cy + s * .4], s * .02, outline=c, width=w)
        d.line([cx - s * .45, cy + s * .42, cx + s * .5, cy + s * .42], fill=c, width=w)
    elif name == "red":
        pts = [(cx + s * .42 * math.cos(a), cy + s * .42 * math.sin(a)) for a in [i * 2 * math.pi / 6 + .3 for i in range(6)]]
        pts.append((cx, cy))
        for i, p in enumerate(pts):
            for q in pts[i + 1:]:
                if rnd.random() < .45 or q == (cx, cy):
                    d.line([p, q], fill=soft if q != (cx, cy) else c, width=max(2, w // 2))
        for p in pts:
            r = s * (.07 if p == (cx, cy) else .045)
            d.ellipse([p[0] - r, p[1] - r, p[0] + r, p[1] + r], outline=c, width=w)
    elif name == "escudo":
        pts = [(cx, cy - s * .45), (cx + s * .36, cy - s * .3), (cx + s * .3, cy + s * .12), (cx, cy + s * .45),
               (cx - s * .3, cy + s * .12), (cx - s * .36, cy - s * .3)]
        d.line(pts + [pts[0]], fill=c, width=w, joint="curve")
        d.line([(cx - s * .14, cy), (cx - s * .02, cy + s * .12), (cx + s * .16, cy - s * .12)], fill=c, width=w, joint="curve")
    elif name == "calendario":
        d.rounded_rectangle([cx - s * .4, cy - s * .34, cx + s * .4, cy + s * .4], s * .05, outline=c, width=w)
        d.line([cx - s * .4, cy - s * .16, cx + s * .4, cy - s * .16], fill=c, width=w)
        for k in (-.2, .2):
            d.line([cx + s * k, cy - s * .44, cx + s * k, cy - s * .26], fill=c, width=w)
        for r_ in range(3):
            for k in range(4):
                x, y = cx - s * .27 + k * s * .18, cy - s * .02 + r_ * s * .14
                d.rounded_rectangle([x - s * .04, y - s * .035, x + s * .04, y + s * .035], s * .01,
                                    fill=c if (r_, k) == (1, 2) else None, outline=soft, width=max(2, w // 2))
    elif name == "voz":
        for i in range(13):
            h = s * .42 * abs(math.sin(i * .9 + rnd.random())) + s * .05
            x = cx - s * .45 + i * s * .075
            d.line([x, cy - h, x, cy + h], fill=c, width=w)
    elif name == "carpeta":
        d.polygon([(cx - s * .42, cy - s * .3), (cx - s * .12, cy - s * .3), (cx - s * .05, cy - s * .2),
                   (cx + s * .42, cy - s * .2), (cx + s * .42, cy + s * .34), (cx - s * .42, cy + s * .34)], outline=c, width=w)
        d.line([cx - s * .42, cy - s * .1, cx + s * .42, cy - s * .1], fill=soft, width=w)


def cmd_hero(args):
    from PIL import Image, ImageDraw, ImageFilter
    W, H = 1600, 860
    rnd = random.Random(args.version + (args.motif or ""))
    img = Image.new("RGB", (W, H))
    px = ImageDraw.Draw(img)
    for y in range(H):  # degradado de la marca
        px.line([(0, y), (W, y)], fill=_mix(PALETTE["bg0"], PALETTE["bg1"], y / H * .9))

    glow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    g = ImageDraw.Draw(glow)
    for _ in range(7):  # manchas de luz
        r = rnd.randint(160, 420)
        x, y = rnd.randint(-100, W + 100), rnd.randint(-100, H + 100)
        col = rnd.choice([PALETTE["p500"], PALETTE["p400"], PALETTE["a500"], PALETTE["a400"]])
        g.ellipse([x - r, y - r, x + r, y + r], fill=col + (rnd.randint(70, 130),))
    glow = glow.filter(ImageFilter.GaussianBlur(110))
    img = Image.alpha_composite(img.convert("RGBA"), glow)

    lines = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ld = ImageDraw.Draw(lines)
    for k in range(16):  # ondas
        amp, freq, ph = rnd.uniform(20, 90), rnd.uniform(.002, .006), rnd.uniform(0, 6.3)
        base = H * .2 + k * H * .045
        col = _mix(PALETTE["p300"], PALETTE["a300"], k / 16) + (38 + k * 3,)
        pts = [(x, base + amp * math.sin(x * freq + ph) + 30 * math.sin(x * freq * 2.7 + ph * 1.3)) for x in range(0, W + 20, 16)]
        ld.line(pts, fill=col, width=2)
    for _ in range(90):  # puntos
        x, y, r = rnd.randint(0, W), rnd.randint(0, H), rnd.choice([1, 2, 2, 3])
        ld.ellipse([x - r, y - r, x + r, y + r], fill=(255, 255, 255, rnd.randint(40, 120)))
    img = Image.alpha_composite(img, lines)

    fg = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    fd = ImageDraw.Draw(fg)
    motif = args.motif or "abstracto"
    if motif != "abstracto":
        fd.ellipse([W * .72 - 250, H * .5 - 250, W * .72 + 250, H * .5 + 250], fill=(255, 255, 255, 18), outline=(255, 255, 255, 50), width=3)
        _motif(fd, motif, int(W * .72), int(H * .5), 380, rnd)
    else:
        for i in range(5):
            r = 90 + i * 55
            fd.arc([W * .72 - r, H * .5 - r, W * .72 + r, H * .5 + r], rnd.randint(0, 360), rnd.randint(0, 360) + 140,
                   fill=(255, 255, 255, 170 - i * 25), width=6)
    fd.rounded_rectangle([96, 300, 96 + 12, 560], 6, fill=PALETTE["a400"] + (255,))
    if args.kicker:
        fd.text((136, 300), args.kicker, font=_font(46, bold=False), fill=(255, 255, 255, 220))
    size = 150
    while size > 60 and fd.textlength(args.version, font=_font(size)) > 760:
        size -= 6
    fd.text((130, 360 + (150 - size) // 2), args.version, font=_font(size), fill=(255, 255, 255, 255))
    if args.caption:
        fd.text((136, 540), args.caption, font=_font(32, bold=False), fill=PALETTE["a300"] + (235,))
    img = Image.alpha_composite(img, fg).convert("RGB")
    img.save(args.out, optimize=True)
    print(args.out)


DEFAULTS = {"status": "published", "author": "Iurefficient", "category": "Novedades de versión", "brand": "derecho"}


def cmd_publish(args):
    with open(args.article, encoding="utf-8") as f:
        a = json.load(f)
    for k in ("title", "slug", "date", "excerpt", "body_html"):
        if not a.get(k):
            raise SystemExit(f"Falta el campo «{k}» en {args.article}")
    for k in ("title", "excerpt", "body_html"):
        if not (a.get("en") or {}).get(k):
            raise SystemExit(f"Falta en.{k} (versión en inglés) en {args.article}")
    cms = connect()
    if cms.exists(a["slug"]):
        raise SystemExit(f"Ya existe un artículo con slug {a['slug']}; no publico dos veces")
    if a.get("image_file"):
        a["image"] = cms.upload(a["image_file"])
    inline_images(cms, a)
    slug = cms.save_article({**DEFAULTS, **a})
    print(json.dumps({"slug": slug, "url": f"{BASE}/articulos/{slug}", "image": a.get("image", "")}, ensure_ascii=False))


def inline_images(cms, a):
    """Sube `images` ({"IMG1": "ruta/local.png"}) y cambia cada marcador por la URL publicada en el cuerpo ES y EN."""
    for key, path in (a.pop("images", None) or {}).items():
        url = "/" + cms.upload(path).lstrip("/")
        for src in (a, a.get("en") or {}):
            if key not in src.get("body_html", ""):
                raise SystemExit(f"El marcador {key} no aparece en body_html ({'en' if src is not a else 'es'})")
            src["body_html"] = src["body_html"].replace(key, url)


def cmd_update(args):
    """Modifica un artículo existente: sólo cambian las claves presentes en el JSON (slug incluido)."""
    with open(args.article, encoding="utf-8") as f:
        a = json.load(f)
    cms = connect()
    if a.get("image_file"):
        a["image"] = cms.upload(a["image_file"])
    inline_images(cms, a)
    slug = cms.save_article(a, slug=args.slug)
    print(json.dumps({"slug": slug, "url": f"{BASE}/articulos/{slug}"}, ensure_ascii=False))


PLAN = os.path.join(os.path.dirname(os.path.abspath(__file__)), "mejores_practicas.json")


def cmd_practicas(args):
    """¿Toca artículo de mejores prácticas? Rota sectores y toma el primer tema sin publicar del siguiente."""
    import datetime
    with open(PLAN, encoding="utf-8") as f:
        plan = json.load(f)
    cms = connect()
    existing = {s for s, _, _ in cms.articles()}
    done = []
    for i, t in enumerate(plan["temas"]):
        if t["slug"] in existing:
            done.append({**t, "fecha": cms.date_of(t["slug"]), "_i": i})
    today = datetime.date.today()
    last = max(done, key=lambda t: (t["fecha"], t["_i"])) if done else None
    days = (today - datetime.date.fromisoformat(last["fecha"])).days if last and last["fecha"] else None
    rot = plan["rotacion"]
    start = (rot.index(last["sector"]) + 1) % len(rot) if last else 0
    nxt = None
    for k in range(len(rot)):
        sector = rot[(start + k) % len(rot)]
        nxt = next((t for t in plan["temas"] if t["sector"] == sector and t["slug"] not in existing), None)
        if nxt:
            nxt = {**nxt, "nombre_sector": plan["sectores"][sector]["nombre"], "cabecera": plan["sectores"][sector]["cabecera"],
                   "categoria": plan["categoria"]}
            break
    due = nxt is not None and (days is None or days >= plan["cada_dias"])
    print(json.dumps({
        "toca": due,
        "dias_desde_el_ultimo": days,
        "ultimo": {k: last[k] for k in ("slug", "sector", "fecha")} if last else None,
        "publicados": len(done), "pendientes": len(plan["temas"]) - len(done),
        "siguiente": nxt,
    }, ensure_ascii=False, indent=2))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("status")
    s.add_argument("--repo", required=True, help="clon de ellaguno/expert-collaborator")
    h = sub.add_parser("hero")
    h.add_argument("--version", required=True, help="texto grande, p. ej. v4.85 o PMO")
    h.add_argument("--kicker", default="¿Qué hay de nuevo?", help="rótulo sobre el texto grande (vacío = sin rótulo)")
    h.add_argument("--caption", default="", help="frase corta bajo la versión")
    h.add_argument("--motif", choices=MOTIFS, default="abstracto")
    h.add_argument("--out", required=True)
    p = sub.add_parser("publish")
    p.add_argument("article", help="JSON con title, slug, date, excerpt, body_html, tags, image_file y en{title, excerpt, body_html, seo_desc, tags}")
    u = sub.add_parser("update")
    u.add_argument("slug", help="slug actual del artículo")
    u.add_argument("article", help="JSON con sólo los campos a cambiar (mismo formato que publish)")
    sub.add_parser("practicas", help="¿toca artículo de mejores prácticas y cuál?")
    args = ap.parse_args()
    {"status": cmd_status, "hero": cmd_hero, "publish": cmd_publish, "update": cmd_update, "practicas": cmd_practicas}[args.cmd](args)


if __name__ == "__main__":
    main()
