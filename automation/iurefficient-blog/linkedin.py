#!/usr/bin/env python3
"""Publica en el perfil de LinkedIn de Eduardo con la API oficial (Share on LinkedIn).

Subcomandos:
  whoami                          Comprueba el token: nombre y URN del perfil (sólo lectura).
  post POST.json [--dry-run]      Publica un texto con la tarjeta de un artículo.

POST.json: {"text": "...", "url": "https://iurefficient.com/articulos/...",
            "title": "...", "description": "...", "image_file": "hero.png"}

Token: variable LINKEDIN_ACCESS_TOKEN (permisos openid, profile, w_member_social; dura 60 días).
Versión de la API: LINKEDIN_VERSION (AAAAMM); por omisión el mes anterior, y si LinkedIn la rechaza
se prueban los meses previos.
"""
import argparse
import datetime
import json
import os
import re
import sys
import urllib.error
import urllib.request

API = "https://api.linkedin.com"


def token():
    t = os.environ.get("LINKEDIN_ACCESS_TOKEN", "").strip()
    if not t:
        raise SystemExit("Falta LINKEDIN_ACCESS_TOKEN en el entorno")
    return t


def versions():
    if os.environ.get("LINKEDIN_VERSION"):
        return [os.environ["LINKEDIN_VERSION"]]
    d = datetime.date.today().replace(day=1)
    out = []
    for _ in range(6):
        d = (d - datetime.timedelta(days=1)).replace(day=1)
        out.append(d.strftime("%Y%m"))
    return out


def call(method, url, body=None, headers=None, raw=False):
    h = {"Authorization": f"Bearer {token()}", **(headers or {})}
    data = body if raw or body is None else json.dumps(body).encode()
    if body is not None and not raw:
        h["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, method=method, headers=h)
    try:
        with urllib.request.urlopen(req) as r:
            txt = r.read().decode("utf-8", "replace")
            return r.status, dict(r.headers), (json.loads(txt) if txt.strip().startswith("{") else txt)
    except urllib.error.HTTPError as e:
        txt = e.read().decode("utf-8", "replace")
        if e.code == 401:
            raise SystemExit("LinkedIn rechazó el token (401): caducó o fue revocado. Hay que generar uno nuevo.")
        try:
            return e.code, dict(e.headers), json.loads(txt)
        except ValueError:
            return e.code, dict(e.headers), txt


def rest(method, path, body=None, raw=False, ctype=None):
    """Llama a /rest/ probando versiones hasta dar con una vigente."""
    last = None
    for v in versions():
        h = {"LinkedIn-Version": v, "X-Restli-Protocol-Version": "2.0.0"}
        if ctype:
            h["Content-Type"] = ctype
        code, hdrs, j = call(method, API + path, body, h, raw)
        msg = json.dumps(j) if isinstance(j, dict) else str(j)
        if code in (400, 426) and re.search(r"version", msg, re.I):
            last = (code, msg)
            continue
        return code, hdrs, j
    raise SystemExit(f"Ninguna versión de la API funcionó: {last}")


def me():
    code, _, j = call("GET", API + "/v2/userinfo")
    if code != 200:
        raise SystemExit(f"No pude leer el perfil ({code}): {j}")
    return j


def little_text(s):
    """Escapa el texto al formato «little text» de LinkedIn y convierte #Etiqueta en hashtag."""
    tags = {}

    def keep(m):
        k = f"\x00{len(tags)}\x00"
        tags[k] = "{hashtag|\\#|" + m.group(1) + "}"
        return k

    s = re.sub(r"(?<![\w#])#(\w+)", keep, s)
    s = re.sub(r"([\\|{}@\[\]()<>#*_~])", r"\\\1", s)
    for k, v in tags.items():
        s = s.replace(k, v)
    return s


def upload_image(author, path):
    code, _, j = rest("POST", "/rest/images?action=initializeUpload",
                      {"initializeUploadRequest": {"owner": author}})
    if code != 200:
        raise SystemExit(f"No pude iniciar la subida de la imagen ({code}): {j}")
    val = j["value"]
    with open(path, "rb") as f:
        req = urllib.request.Request(val["uploadUrl"], data=f.read(), method="PUT",
                                     headers={"Authorization": f"Bearer {token()}"})
    with urllib.request.urlopen(req) as r:
        if r.status not in (200, 201):
            raise SystemExit(f"La subida de la imagen respondió {r.status}")
    return val["image"]


def cmd_whoami(args):
    j = me()
    print(json.dumps({"nombre": j.get("name"), "urn": f"urn:li:person:{j.get('sub')}",
                      "api_version": versions()[0]}, ensure_ascii=False))


def cmd_post(args):
    with open(args.post, encoding="utf-8") as f:
        p = json.load(f)
    for k in ("text", "url", "title"):
        if not p.get(k):
            raise SystemExit(f"Falta «{k}» en {args.post}")
    author = f"urn:li:person:{me()['sub']}"
    body = {
        "author": author,
        "commentary": little_text(p["text"]),
        "visibility": "PUBLIC",
        "distribution": {"feedDistribution": "MAIN_FEED", "targetEntities": [], "thirdPartyDistributionChannels": []},
        "content": {"article": {"source": p["url"], "title": p["title"], "description": p.get("description", "")}},
        "lifecycleState": "PUBLISHED",
        "isReshareDisabledByAuthor": False,
    }
    if args.dry_run:
        print(json.dumps(body, ensure_ascii=False, indent=2))
        return
    if p.get("image_file"):
        body["content"]["article"]["thumbnail"] = upload_image(author, p["image_file"])
    code, hdrs, j = rest("POST", "/rest/posts", body)
    if code != 201:
        raise SystemExit(f"LinkedIn no publicó ({code}): {j}")
    urn = hdrs.get("x-restli-id") or hdrs.get("X-RestLi-Id") or ""
    print(json.dumps({"urn": urn, "url": f"https://www.linkedin.com/feed/update/{urn}/"}, ensure_ascii=False))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("whoami")
    p = sub.add_parser("post")
    p.add_argument("post")
    p.add_argument("--dry-run", action="store_true", help="muestra lo que se enviaría, sin publicar")
    args = ap.parse_args()
    {"whoami": cmd_whoami, "post": cmd_post}[args.cmd](args)


if __name__ == "__main__":
    main()
