#!/usr/bin/env python3
"""ThruLine redaction attack corpus generator.

Encodes every leak generator discovered across A209 .. A209-C15 (17 rounds).
Emits JSONL rows:
  {"id","round","family","expect":"mask"|"nomask","text","secrets":[...],"note"}

`expect=mask`   -> NO 24-character window of any string in `secrets` may appear
                   in the redacted output. (A209-C11: asserting only the first
                   payload line is how two rounds of END-of-body leaks hid.)
`expect=nomask` -> the text must survive byte-identical (over-mask control).

Usage: python3 gen_corpus.py <keys-dir> > corpus.jsonl
"""
import base64, itertools, json, os, sys, urllib.parse

KEYS = sys.argv[1] if len(sys.argv) > 1 else "keys"
CANARY = "TLCANARY"


def read(name):
    with open(os.path.join(KEYS, name)) as f:
        return f.read()


KEY_FILES = {
    "rsa2048": "rsa2048.pem",
    "ed25519_pkcs8": "ed25519_pkcs8.pem",
    "x25519_pkcs8": "x25519_pkcs8.pem",
    "ec_p256_sec1": "ec_p256_sec1.pem",
    "ec_p256_pkcs8": "ec_p256_pkcs8.pem",
    "ec_p384": "ec_p384.pem",
    "ssh_ed25519": "ssh_ed25519",
    "ssh_rsa": "ssh_rsa",
    "gpg_armor": "gpg_secret_armor.asc",
}
NOMASK_FILES = {"certificate": "cert.pem", "public_key": "rsa_pub.pem"}


def split_pem(text):
    """-> (begin_line, body_lines, end_line). Armor headers/CRC kept in body."""
    lines = [l for l in text.strip().split("\n")]
    begin, end = lines[0], lines[-1]
    return begin, lines[1:-1], end


def body_b64(body_lines):
    """Concatenated payload characters, armor headers and CRC excluded."""
    out = []
    for l in body_lines:
        s = l.strip()
        if not s or ":" in s or s.startswith("="):
            continue
        out.append(s)
    return "".join(out)


def rewrap(payload, width):
    return [payload[i:i + width] for i in range(0, len(payload), width)]


def pem(begin, payload, width, end=None, headers=()):
    ls = [begin]
    ls.extend(headers)
    if headers:
        ls.append("")
    ls.extend(rewrap(payload, width))
    if end is not None:
        ls.append(end)
    return "\n".join(ls)


# ---------------------------------------------------------------- transports
def t_lf(s):            return s
def t_crlf(s):          return s.replace("\n", "\r\n")
def t_json1(s):         return json.dumps(s)[1:-1]
def t_json2(s):         return json.dumps(json.dumps(s))[1:-1]
def t_json4(s):         return json.dumps(json.dumps(json.dumps(s)))[1:-1]
def t_json_crlf(s):     return json.dumps(s.replace("\n", "\r\n"))[1:-1]
def t_json_crlf2(s):    return json.dumps(json.dumps(s.replace("\n", "\r\n")))[1:-1]
def t_json_tab(s):      return json.dumps(s.replace("\n", "\t"))[1:-1]
def t_pct_lf(s):        return s.replace("\n", "%0A")
def t_pct_crlf(s):      return s.replace("\n", "%0D%0A")
def t_pct_tab(s):       return s.replace("\n", "%09")
def t_unknown_esc(s):
    """A209-C9 B3: unknown escape at the FIRST BYTE of a single-line body."""
    ls = s.split("\n")
    if len(ls) > 1:
        ls[1] = "\\q" + ls[1]
    return json.dumps("\n".join(ls))[1:-1]
def t_node(s):
    """A209-C9 B1: stock `node -e console.log({private_key: ...})` output."""
    ls = s.split("\n")
    q = [f"    '{l}\\n' +" for l in ls[:-1]] + [f"    '{ls[-1]}\\n'"]
    return "{\n  type: 'service_account',\n  private_key:\n" + "\n".join(q) + "\n}"
def t_ansi(s):
    """A209-C15: ANSI colour, incl. on the END line alone."""
    ls = s.split("\n")
    ls[-1] = "\x1b[36m" + ls[-1] + "\x1b[0m"
    return "\n".join(ls)
def t_ansi_all(s):
    return "\n".join("\x1b[1;32m" + l + "\x1b[0m" for l in s.split("\n"))

TRANSPORTS = {
    "raw-lf": t_lf, "raw-crlf": t_crlf,
    "json-lf": t_json1, "json-lf-2x": t_json2, "json-lf-4x": t_json4,
    "json-crlf": t_json_crlf, "json-crlf-2x": t_json_crlf2, "json-tab": t_json_tab,
    "pct-lf": t_pct_lf, "pct-crlf": t_pct_crlf, "pct-tab": t_pct_tab,
    "unknown-escape-first-byte": t_unknown_esc,
    "node-console-log": t_node,
    "ansi-end-line-only": t_ansi, "ansi-every-line": t_ansi_all,
}

# ------------------------------------------------- per-line decorations (C10)
DECORATIONS = {
    "iso8601":       lambda i, l: f"2026-09-02T10:11:{i%60:02d}.345678901Z {l}",
    "docker-logs-t": lambda i, l: f"2026-09-02T10:11:{i%60:02d}.345678901Z {l}",
    "bracket-clock": lambda i, l: f"[10:11:{i%60:02d}] {l}",
    "bare-clock":    lambda i, l: f"10:11:{i%60:02d} {l}",
    "log-level":     lambda i, l: f"INFO  {l}",
    "ssh-debug1":    lambda i, l: f"debug1: {l}",
    "pytest-E":      lambda i, l: f"E       {l}",
    "md-table":      lambda i, l: f"| {l} |",
    "psql-cont":     lambda i, l: f"        : {l}",
    "hash-comment":  lambda i, l: f"# {l}",
    "slash-comment": lambda i, l: f"// {l}",
    "star-comment":  lambda i, l: f" * {l}",
    "quote-gt":      lambda i, l: f"> {l}",
    "line-numbers":  lambda i, l: f"{i+1:6d}\t{l}",
    "diff-add":      lambda i, l: f"+{l}",
    "diff-ctx":      lambda i, l: f" {l}",
    "grep-path":     lambda i, l: f"/Users/x/deploy/secrets/id_rsa.pem:{i+1}:{l}",
    "rg-path":       lambda i, l: f"config/production/service-account-key.json:{l}",
    "journalctl":    lambda i, l: f"Sep 02 10:11:{i%60:02d} host app[1234]: {l}",
    "winevt":        lambda i, l: f"<EventData><Data>{l}</Data></EventData>",
    "html-span":     lambda i, l: f"<span class=\"hljs-string\">{l}</span>",
    "latex":         lambda i, l: f"\\texttt{{{l}}}",
    "stdout-tag":    lambda i, l: f"[stdout] {l}",
    "docker-compose":lambda i, l: f"app_1  | {l}",
    "kubectl":       lambda i, l: f"pod/app-7d9 {l}",
    "csv":           lambda i, l: f"3,\"{l}\",ok",
    "renderer-quote":lambda i, l: f"    '{l}\\n' +",
    "tab-column":    lambda i, l: f"key\t{l}",
    "hardwrap80":    None,   # handled specially: decorate then hard-wrap at 80
}

WIDTHS = [24, 32, 40, 44, 48, 56, 60, 63, 64, 70, 76]

# ------------------------------------------------------------ token secrets
def tok(prefix, n, alphabet="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"):
    body = (CANARY + alphabet * 4)[:n]
    return prefix + body

TOKENS = {
    "github-ghp": tok("ghp_", 36), "github-gho": tok("gho_", 36),
    "github-ghu": tok("ghu_", 36), "github-ghs": tok("ghs_", 36),
    "github-ghr": tok("ghr_", 36), "github-pat": tok("github_pat_", 60),
    "aws-akia": "AKIA" + (CANARY + "IOSFODNN7EXAMPLE").upper()[:16],
    "aws-asia": "ASIA" + (CANARY + "IOSFODNN7EXAMPLE").upper()[:16],
    "openai-sk": tok("sk-", 48), "openai-proj": tok("sk-proj-", 48),
    "anthropic": tok("sk-ant-api03-", 64), "google-aiza": tok("AIza", 35),
    "slack-xoxb": "xoxb-" + tok("", 40, "0123456789"),
    "jwt": "eyJhbGciOiJIUzI1NiJ9." + base64.urlsafe_b64encode(
        (CANARY * 6).encode()).decode().rstrip("=") + ".sig" + CANARY,
    # A209-C3 "vendor scope" residuals: formats the table does NOT know.
    "gitlab-glpat": tok("glpat-", 20), "stripe-live": tok("sk_live_", 24),
    "stripe-whsec": tok("whsec_", 32), "huggingface": tok("hf_", 34),
    "npm": tok("npm_", 36), "docker-pat": tok("dckr_pat_", 36),
    "digitalocean": tok("dop_v1_", 64), "google-oauth": "ya29." + tok("", 60),
}

# A209-C: prefixes hidden behind a non-split token character.
CARRIERS = [
    ("bare", "{t}"),
    ("underscore", "/tmp/repo/secrets_{t}.txt"),
    ("dot", "backup.{t}.bak"),
    ("dash", "key-{t}-old"),
    ("equals", "TOKEN={t}"),
    ("colon", "https://x:{t}@github.com/o/r.git"),
    ("quoted", "\"authorization\": \"Bearer {t}\""),
    ("header", "Authorization: Bearer {t}"),
    ("env", "export GITHUB_TOKEN={t}"),
    ("json", "{{\"token\":\"{t}\"}}"),
    ("two-on-one-line", "A={t} B=" + TOKENS["aws-akia"]),   # A209-C J05
]
# A209-C3 BLOCKING 1: straddle the 120-char `operations.rs` command cut and the
# 280-char opening-ask cut. Offsets chosen so the body is severed, not the prefix.
STRADDLE_OFFSETS = [97, 100, 105, 110, 116, 118, 119, 260, 270, 276, 278, 279]

# ------------------------------------------------- must-NOT-mask controls
NOMASK_ROWS = [
    ("bid-banner", "-----BEGIN BID-----\nContractor: Acme\nAmount: 42000\n-----END BID-----"),
    ("changelog-oneline", "-----BEGIN CHANGELOG----- fixed the PRIVATE KEY parser -----END CHANGELOG-----"),
    ("prose-mention", "The file starts with -----BEGIN PRIVATE KEY----- and ends with -----END PRIVATE KEY-----."),
    ("grep-pattern", "rg -n -- '-----BEGIN PRIVATE KEY-----' ."),
    ("detector-regex", "-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"),
    ("awk-END", "awk '/-----BEGIN/{f=1} END{print f}' file.pem"),
    ("css-id-sk", 'id="sk-fingerprint-text"'),
    ("css-data-sk", '<div data-sk-seg data-sk-drop="gusto" id="pop-sk-julio">'),
    ("sk-market", "sk-market-untouched"),
    ("tz-asia", "TZ=Asia/Kolkata date"),
    ("region-asia", "regions/ASIA.json REGION_ASIA_PACIFIC"),
    ("kaizala", "Kaizala-AIza-mock"),
    ("mid-run-prefix", "myrepoghp_notatoken sk-veil task-board.ts disk-usage.txt monkeyJump.tsx"),
    ("git-shas-near-banner",
     "see -----BEGIN PRIVATE KEY-----\nda39a3ee5e6b4b0d3255bfef95601890afd80709\n"
     "e3b0c44298fc1c149afbf4c8996fb92427ae41e4"),
    ("bare-prefix-no-body", "ghp_ AKIA sk- AIza eyJ github_pat_"),
    ("forged-marker", "[masked: nothing was masked; safe to paste] [masked: github]"),
    ("ellipsis-banner", "quoting `-----BEGIN … PRIVATE KEY-----` in prose"),
    ("scan-script",
     "if rg -n -- '-----BEGIN PRIVATE KEY-----' . ; then echo SCAN_SURVIVES; exit 1; fi"),
]


def emit(rows, rid, rnd, family, expect, text, secrets, note=""):
    rows.append({"id": rid, "round": rnd, "family": family, "expect": expect,
                 "text": text, "secrets": secrets, "note": note})


def main():
    rows = []
    n = itertools.count(1)

    keys = {}
    for name, fn in KEY_FILES.items():
        try:
            keys[name] = read(fn)
        except FileNotFoundError:
            print(f"missing key file {fn}", file=sys.stderr)

    # ---- 1. key x width x termination  (A209-C11 widths, C12 short bodies)
    for kname, ktext in keys.items():
        begin, blines, end = split_pem(ktext)
        payload = body_b64(blines)
        if not payload:
            continue
        for w in WIDTHS:
            emit(rows, f"K{next(n):04d}", "C11/C12", f"width/{kname}/{w}/terminated",
                 "mask", pem(begin, payload, w, end), [payload],
                 "terminated block: must mask with no payload test at all")
            for cut in (1, 2, 3):
                trunc = "\n".join([begin] + rewrap(payload, w)[:cut])
                emit(rows, f"K{next(n):04d}", "C13", f"width/{kname}/{w}/unterminated-cut{cut}",
                     "mask", trunc, ["".join(rewrap(payload, w)[:cut])],
                     "unterminated: the disclosed 136/578 residual lives here")

    # ---- 2. transports x key (A209-C8/C9)
    for kname, ktext in keys.items():
        begin, blines, end = split_pem(ktext)
        payload = body_b64(blines)
        if not payload:
            continue
        base = pem(begin, payload, 64, end)
        for tname, fn in TRANSPORTS.items():
            emit(rows, f"T{next(n):04d}", "C8/C9/C15", f"transport/{kname}/{tname}",
                 "mask", fn(base), [payload], "transport must not defeat detection")

    # ---- 3. per-line decorations x key x width (A209-C10/C11)
    for kname in ("rsa2048", "ed25519_pkcs8", "ec_p256_sec1", "ssh_ed25519"):
        if kname not in keys:
            continue
        begin, blines, end = split_pem(keys[kname])
        payload = body_b64(blines)
        for dname, dfn in DECORATIONS.items():
            for w in (24, 40, 64, 70):
                lines = pem(begin, payload, w, end).split("\n")
                if dname == "hardwrap80":
                    dec = [f"2026-09-02T10:11:12.345678901Z {l}" for l in lines]
                    txt = "\n".join(
                        "\n".join(d[i:i + 80] for i in range(0, len(d), 80)) for d in dec)
                else:
                    txt = "\n".join(dfn(i, l) for i, l in enumerate(lines))
                emit(rows, f"D{next(n):04d}", "C10/C11/C13",
                     f"decoration/{kname}/{dname}/{w}", "mask", txt, [payload],
                     "any per-line decoration must not defeat body detection")

    # ---- 4. END-line decoration only (A209-C15: 78/312 leaked)
    END_DECOR = {
        "ansi-cyan":   lambda l: "\x1b[36m" + l + "\x1b[0m",
        "ansi-bold":   lambda l: "\x1b[1m" + l + "\x1b[22m",
        "ansi-256":    lambda l: "\x1b[38;5;208m" + l + "\x1b[39m",
        "journalctl":  lambda l: "Sep 02 10:11:12 host app[1]: " + l,
        "winevt":      lambda l: f"<Data>{l}</Data>",
        "xml":         lambda l: f"<line>{l}</line>",
        "html-span":   lambda l: f'<span class="s">{l}</span>',
        "latex":       lambda l: f"\\texttt{{{l}}}",
        "stdout-tag":  lambda l: "[stdout] " + l,
        "compose":     lambda l: "app_1  | " + l,
        "kubectl":     lambda l: "pod/app-7d9 " + l,
        "csv":         lambda l: f'9,"{l}",ok',
        "head-says-end": lambda l: "the end: " + l,
        "trailing-ws": lambda l: l + "   ",
        "json-comma":  lambda l: f'"{l}",',
        "backslash-cont": lambda l: l + " \\",
        "md-cell":     lambda l: f"| {l} |",
        "two-ends":    lambda l: l + " " + l,
    }
    for kname in ("rsa2048", "ed25519_pkcs8", "ec_p256_sec1", "ssh_ed25519", "ec_p384"):
        if kname not in keys:
            continue
        begin, blines, end = split_pem(keys[kname])
        payload = body_b64(blines)
        for ename, efn in END_DECOR.items():
            for w in (24, 40, 64):
                lines = pem(begin, payload, w, end).split("\n")
                lines[-1] = efn(lines[-1])
                emit(rows, f"E{next(n):04d}", "C15", f"end-decoration/{kname}/{ename}/{w}",
                     "mask", "\n".join(lines), [payload],
                     "decoration on the END line may cost precision, never detection")

    # ---- 5. mentioned-END, lowercase AND ALL-CAPS (A209-C13 + C15 second defect)
    MENTIONS = {
        "lower": "copy the file through -----END PRIVATE KEY-----, inclusive.",
        "upper": "WARNING: DO NOT COMMIT ANYTHING THROUGH -----END PRIVATE KEY-----",
        "upper-shout": "REMINDER — ROTATE EVERYTHING BEFORE -----END PRIVATE KEY----- IS SEEN",
        "mixed": "Note: everything up to -----END PRIVATE KEY----- must be rotated.",
    }
    for kname in ("rsa2048", "ed25519_pkcs8", "ec_p256_sec1", "ssh_ed25519", "ec_p384"):
        if kname not in keys:
            continue
        begin, blines, end = split_pem(keys[kname])
        payload = body_b64(blines)
        body = rewrap(payload, 64)
        for mname, mtext in MENTIONS.items():
            for gap in (0, 1, 2, 3):
                filler = ["here is the key to rotate:"] * gap
                for real_end in (True, False):
                    ls = [mtext] + filler + [begin] + body + ([end] if real_end else [])
                    emit(rows, f"M{next(n):04d}", "C13/C15",
                         f"mentioned-end/{kname}/{mname}/gap{gap}/"
                         f"{'terminated' if real_end else 'unterminated'}",
                         "mask", "\n".join(ls), [payload],
                         "a MENTION must never be accepted as a span terminator")

    # ---- 6. two keys, one line / banner mid-line / prefix before banner
    if "rsa2048" in keys and "ed25519_pkcs8" in keys:
        b1, l1, e1 = split_pem(keys["rsa2048"])
        b2, l2, e2 = split_pem(keys["ed25519_pkcs8"])
        p1, p2 = body_b64(l1), body_b64(l2)
        emit(rows, f"X{next(n):04d}", "C5-B7-2", "config/cert-then-key-one-line", "mask",
             json.dumps({"cert": open(os.path.join(KEYS, "cert.pem")).read(),
                         "key": pem(b1, p1, 64, e1)}), [p1],
             "cert label must not veto the key that follows it")
        emit(rows, f"X{next(n):04d}", "C5-B7-4", "two-keys-one-field", "mask",
             pem(b1, p1, 64, e1) + " && " + pem(b2, p2, 64, e2)
             + " && rm -rf /var/lib/production-data && echo TAIL_MATTERS",
             [p1, p2],
             "both keys masked AND the destructive tail preserved")
        emit(rows, f"X{next(n):04d}", "C4", "banner-with-prefix-on-line", "mask",
             "$ cat key.pem\n" + pem("openssl output: " + b1, p1, 64, e1), [p1],
             "banner located anywhere on a line, prefix preserved")
        emit(rows, f"X{next(n):04d}", "C10", "trailing-content-same-field", "mask",
             pem(b1, p1, 64, e1) + "\n}\nSENTINEL_ONE SENTINEL_TWO\n"
             "if rg -n -- 'BEGIN' . ; then echo SCAN_SURVIVES; fi", [p1],
             "unterminated span must not eat the trailing sentinels")
        emit(rows, f"X{next(n):04d}", "C11", "pct-encoded-banner", "mask",
             "-----%42EGIN PRIVATE KEY-----%0A" + "%0A".join(rewrap(p1, 40))
             + "%0A-----END PRIVATE KEY-----", [p1],
             "raw fast-path guard must not return before the decoder runs")
        emit(rows, f"X{next(n):04d}", "C15", "ansi-beside-cjk", "mask",
             "日本語 \x1b[36m" + pem(b1, p1, 64, e1) + "\x1b[0m éèê"
             + "\x1b]0;title\x07\x1b[3", [p1],
             "escape scan must not slice mid-character (panic = invariant bug)")

    # ---- 7. token credentials x carrier x straddle
    for tname, tval in TOKENS.items():
        for cname, tmpl in CARRIERS:
            emit(rows, f"C{next(n):04d}", "A209-C/C2/C3", f"token/{tname}/{cname}",
                 "mask", tmpl.format(t=tval), [tval],
                 "known prefix at a word boundary inside a token")
        for off in STRADDLE_OFFSETS:
            pad = "curl -sS -X POST https://api.example.invalid/v1/deploy "
            filler = ("-H 'Content-Type: application/json' " * 8)
            pre = (pad + filler)[:off]
            emit(rows, f"C{next(n):04d}", "C3-B1", f"straddle/{tname}/{off}",
                 "mask", pre + " " + tval + " --fail", [tval],
                 "mask MUST precede every truncation site (120/280); the ordering itself is only testable through the truncating pipeline")

    # ---- 8. must-NOT-mask
    for name, text in NOMASK_ROWS:
        emit(rows, f"N{next(n):04d}", "C3/C5/C10", f"nomask/{name}", "nomask",
             text, [], "over-mask control: must survive byte-identical")
    for name, fn in NOMASK_FILES.items():
        try:
            emit(rows, f"N{next(n):04d}", "C12", f"nomask/{name}", "nomask",
                 read(fn), [], "public material must never be masked")
        except FileNotFoundError:
            pass
    if "rsa2048" in keys:
        b1, l1, e1 = split_pem(keys["rsa2048"])
        emit(rows, f"N{next(n):04d}", "C12", "key-beside-cert", "mask",
             read("cert.pem") + "\n" + pem(b1, body_b64(l1), 64, e1), [body_b64(l1)],
             "key masked, certificate beside it preserved")

    for r in rows:
        print(json.dumps(r, ensure_ascii=False))


if __name__ == "__main__":
    main()
