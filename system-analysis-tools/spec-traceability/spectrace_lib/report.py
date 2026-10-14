"""Views of the matrix: the README summary, the clause × code grid, the
code → spec index, uncited code, and a self-contained HTML viewer."""
from __future__ import annotations

import html
import json
import os
import re
from collections import Counter, defaultdict

from . import rust
from .matrix import STATUSES, Matrix
from .spec import Spec

# ---------------------------------------------------------------------------
# Generated blocks inside hand-written Markdown


def extract_block(text: str, name: str) -> str | None:
    m = re.search(rf"<!-- spectrace:{name} -->\n(.*?)<!-- /spectrace:{name} -->", text, re.S)
    return m.group(1) if m else None


def replace_block(text: str, name: str, body: str) -> str:
    new = f"<!-- spectrace:{name} -->\n{body.rstrip()}\n<!-- /spectrace:{name} -->"
    return re.sub(rf"<!-- spectrace:{name} -->\n.*?<!-- /spectrace:{name} -->", lambda _: new, text, flags=re.S)


def summary_md(spec: Spec, m: Matrix) -> str:
    from .check import COVERED, coverage

    areas: dict[str, Counter] = {}
    for e in m.clauses():
        areas.setdefault(e.area or "(none)", Counter())[e.status or "?"] += 1
    head = "| Area | Clauses | " + " | ".join(STATUSES) + " |"
    rows = [head, "|" + "---|" * (2 + len(STATUSES))]
    total = Counter()
    for area, c in areas.items():
        total.update(c)
        rows.append(f"| {area} | {sum(c.values())} | " + " | ".join(str(c.get(s, 0) or "") for s in STATUSES) + " |")
    rows.append(f"| **Total** | **{sum(total.values())}** | " + " | ".join(f"**{total.get(s, 0)}**" for s in STATUSES) + " |")
    _, frac, _ = coverage(spec, m)
    text_units = [u for u in spec.units if u.kind == "text"]
    covered = sum(1 for u in text_units if frac[u.index] >= COVERED)
    files = {c.path for c in m.all_citations()}
    tests = {(f, n) for e in m.entries for f, n, _ in e.tests}
    rows.append("")
    rows.append(
        f"Spec coverage: {covered} of {len(text_units)} spec sentences are quoted by a clause entry. "
        f"{len(m.all_citations())} code citations into {len(files)} files; {len(tests)} distinct tests cited."
    )
    return "\n".join(rows) + "\n"


# ---------------------------------------------------------------------------
# Clause × code grid

DEFAULT_GROUPS = [
    ("graph", "src/graph.rs"), ("nodes", "src/nodes/"), ("engine", "src/engine/"), ("formats", "src/formats.rs"),
    ("worker", "src/worker.rs"), ("expr", "src/expr.rs"), ("project", "src/project.rs"),
    ("ui/state", "src/ui/state.rs"), ("ui/session", "src/ui/session.rs"), ("ui/views", "src/ui/"), ("build", "Cargo.toml"), ("test code", "tests/"),
]


def group_of(path: str, by: str) -> str:
    if by == "file":
        return path
    for name, prefix in DEFAULT_GROUPS:
        if path == prefix or (prefix.endswith("/") and path.startswith(prefix)):
            return name
    return "other"


def grid(m: Matrix, by: str = "group") -> tuple[list[str], list[dict]]:
    cols: list[str] = []
    rows = []
    for e in m.clauses():
        cells: dict[str, int] = defaultdict(int)
        for c in e.citations:
            g = group_of(c.path, by)
            cells[g] += c.end - c.start + 1
            if g not in cols:
                cols.append(g)
        rows.append({"id": e.id, "title": e.title, "status": e.status, "cells": dict(cells), "tests": len(e.tests)})
    order = [n for n, _ in DEFAULT_GROUPS] + ["other"] if by != "file" else sorted(cols)
    cols = [c for c in order if c in cols]
    return cols, rows


def grid_md(m: Matrix, by: str = "group") -> str:
    cols, rows = grid(m, by)
    head = "| Clause | Status | " + " | ".join(cols) + " | tests |"
    out = [head, "|" + "---|" * (len(cols) + 3)]
    for r in rows:
        cells = []
        for c in cols:
            n = r["cells"].get(c, 0)
            cells.append("" if not n else ("●" if n >= 20 else "•"))
        out.append(f"| {r['id']} {r['title']} | {r['status']} | " + " | ".join(cells) + f" | {r['tests'] or ''} |")
    out.append("")
    out.append("• cites fewer than 20 lines of that component, ● 20 or more.")
    return "\n".join(out) + "\n"


def grid_csv(m: Matrix, by: str = "group") -> str:
    cols, rows = grid(m, by)
    out = ["id,title,status," + ",".join(cols) + ",tests"]
    for r in rows:
        title = '"' + r["title"].replace('"', '""') + '"'
        out.append(f"{r['id']},{title},{r['status']}," + ",".join(str(r["cells"].get(c, 0)) for c in cols) + f",{r['tests']}")
    return "\n".join(out) + "\n"


# ---------------------------------------------------------------------------
# Code → spec


def code_index(m: Matrix, tree: rust.Tree, dirs=("src",)) -> dict[str, list[dict]]:
    """For each source file, its items with the entries citing them."""
    out: dict[str, list[dict]] = {}
    cites_by_file = defaultdict(list)
    for c in m.all_citations():
        cites_by_file[c.path].append(c)
    for f in tree.files():
        if not f.startswith(tuple(d + "/" for d in dirs)):
            continue
        src = tree.get(f)
        rows = []
        for it in src.items:
            if it.is_test or it.kind in ("impl", "mod"):
                continue
            hits = sorted({c.entry for c in cites_by_file[f] if c.start <= it.end and it.start <= c.end and c.entry})
            rows.append({**it.to_json(), "entries": hits})
        out[f] = rows
    return out


def uncited(m: Matrix, tree: rust.Tree, kinds=("fn",)) -> list[dict]:
    """Per file: functions no entry cites, and the share of function lines
    that are cited."""
    idx = code_index(m, tree)
    cited_lines = defaultdict(set)
    for c in m.all_citations():
        cited_lines[c.path].update(range(c.start, c.end + 1))
    out = []
    for f, rows in idx.items():
        fns = [r for r in rows if r["kind"] in kinds]
        if not fns:
            continue
        lines = set()
        for r in fns:
            lines.update(range(r["start"], r["end"] + 1))
        out.append({
            "file": f, "functions": len(fns), "cited": sum(1 for r in fns if r["entries"]),
            "line_share": round(len(lines & cited_lines[f]) / max(1, len(lines)), 3),
            "uncited": [f"{r['name']} ({r['start']}-{r['end']})" for r in fns if not r["entries"]],
        })
    return out


def uncited_md(rows: list[dict]) -> str:
    out = ["| File | Functions | Cited | Cited share of fn lines |", "|---|---|---|---|"]
    for r in sorted(rows, key=lambda r: r["line_share"]):
        out.append(f"| `{r['file']}` | {r['functions']} | {r['cited']} | {r['line_share']:.0%} |")
    out.append("")
    for r in sorted(rows, key=lambda r: r["file"]):
        if r["uncited"]:
            out.append(f"- `{r['file']}`: " + ", ".join(f"`{u}`" for u in r["uncited"]))
    return "\n".join(out) + "\n"


def why(m: Matrix, tree: rust.Tree, path: str, line: int) -> dict:
    """Which entries depend on `path:line`: citations that include the
    line, and citations elsewhere in the enclosing item."""
    src = tree.get(path)
    item = src.item_at(line) if src else None
    direct = sorted({c.entry for c in m.all_citations() if c.path == path and c.start <= line <= c.end})
    near = []
    if item:
        near = sorted({c.entry for c in m.all_citations() if c.path == path and c.start <= item.end and item.start <= c.end} - set(direct))
    return {"path": path, "line": line, "item": item.name if item else None,
            "item_lines": f"{item.start}-{item.end}" if item else None, "direct": direct, "same_item": near}


# ---------------------------------------------------------------------------
# HTML viewer


def _md_inline(s: str, link_base: str) -> str:
    s = html.escape(s, quote=False)
    s = re.sub(r"`([^`]+)`", r"<code>\1</code>", s)
    s = re.sub(r"\*\*([^*]+)\*\*", r"<b>\1</b>", s)

    def link(mm):
        text, href = mm.group(1), mm.group(2)
        if href.startswith("#") or href.endswith(".md") or ".md#" in href:
            anchor = href.split("#")[-1] if "#" in href else ""
            return f'<a href="#" data-goto="{anchor}">{text}</a>'
        return f'<a href="{html.escape(link_base + href.replace("#L", "#L"))}" target="_blank">{text}</a>'

    return re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", link, s)


def md_to_html(text: str, link_base: str = "") -> str:
    out, items = [], []
    for line in text.splitlines():
        if line.lstrip().startswith(("- ", "* ")):
            items.append(f"<li>{_md_inline(line.lstrip()[2:], link_base)}</li>")
            continue
        if items:
            out.append("<ul>" + "".join(items) + "</ul>")
            items = []
        if line.strip():
            out.append(f"<p>{_md_inline(line, link_base)}</p>")
    if items:
        out.append("<ul>" + "".join(items) + "</ul>")
    return "".join(out)


def html_page(spec: Spec, m: Matrix, tree: rust.Tree, commit: str | None) -> str:
    from .check import coverage

    by_unit, frac, _ = coverage(spec, m)
    entries = []
    for e in m.entries:
        j = e.to_json()
        j["sections_html"] = {k: md_to_html(v) for k, v in e.sections.items()}
        j["quotes_html"] = [_md_inline(q, "") for q, _ in e.quotes]
        entries.append(j)
    units = [{**u.to_json(), "entries": by_unit.get(u.index, []), "covered": round(frac[u.index], 2)} for u in spec.units]
    cols, rows = grid(m, "file")
    from . import ancestry
    anc = ancestry.analyse(m, tree)
    klass = {(v["file"], v["start"]): v["class"] for v in anc["items"].values()}
    code = code_index(m, tree)
    for f, items in code.items():
        for it in items:
            it["ancestry"] = klass.get((f, it["start"]), "")
    data = {
        "commit": commit, "entries": entries, "units": units, "grid": {"cols": cols, "rows": rows},
        "code": code, "statuses": list(STATUSES),
    }
    payload = json.dumps(data, ensure_ascii=False).replace("</", "<\\/")
    return HTML_TEMPLATE.replace("/*DATA*/null", payload)


HTML_TEMPLATE = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Spec Traceability</title>
<style>
:root {
  --bg: #fbfaf7; --panel: #ffffff; --ink: #1d1d1f; --muted: #6b6b70; --line: #e4e1da; --accent: #2f5bd3;
  --implemented: #2e7d4f; --partial: #b7791f; --diverges: #c0392b; --missing: #7d2e8f; --deferred: #6b7280; --process: #2f6f8f;
  --code-bg: #f1efe9;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #17181b; --panel: #1f2125; --ink: #e8e6e1; --muted: #9a9aa2; --line: #2e3036; --accent: #8aa8ff;
    --implemented: #5cc28a; --partial: #e2b04f; --diverges: #ef6f62; --missing: #c58ad6; --deferred: #9aa1ad; --process: #6cb7d6;
    --code-bg: #2a2c31;
  }
}
:root[data-theme="dark"] {
  --bg: #17181b; --panel: #1f2125; --ink: #e8e6e1; --muted: #9a9aa2; --line: #2e3036; --accent: #8aa8ff;
  --implemented: #5cc28a; --partial: #e2b04f; --diverges: #ef6f62; --missing: #c58ad6; --deferred: #9aa1ad; --process: #6cb7d6;
  --code-bg: #2a2c31;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--ink); font: 14px/1.5 -apple-system, "Segoe UI", system-ui, sans-serif; }
header { padding: 16px; border-bottom: 1px solid var(--line); display: flex; flex-wrap: wrap; gap: 12px; align-items: baseline; }
header h1 { font-size: 18px; margin: 0; }
header .muted { color: var(--muted); }
nav { display: flex; gap: 4px; flex-wrap: wrap; }
nav button { border: 1px solid var(--line); background: var(--panel); color: var(--ink); padding: 4px 10px; border-radius: 6px; cursor: pointer; font: inherit; }
nav button.on { border-color: var(--accent); color: var(--accent); }
main { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 0; min-height: calc(100vh - 60px); }
@media (max-width: 860px) { main { grid-template-columns: 1fr; } }
#left, #right { padding: 16px; overflow: auto; }
#left { border-right: 1px solid var(--line); max-height: calc(100vh - 60px); }
#right { max-height: calc(100vh - 60px); }
@media (max-width: 860px) { #left, #right { max-height: none; border-right: 0; } }
code { background: var(--code-bg); padding: 0 3px; border-radius: 3px; font-size: 12.5px; }
a { color: var(--accent); }
.legend { display: flex; flex-wrap: wrap; gap: 10px; font-size: 12px; color: var(--muted); margin-bottom: 10px; }
.dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin-right: 4px; vertical-align: middle; }
.st-Implemented { --c: var(--implemented); } .st-Partial { --c: var(--partial); } .st-Diverges { --c: var(--diverges); }
.st-Missing { --c: var(--missing); } .st-Deferred { --c: var(--deferred); } .st-Process { --c: var(--process); } .st-none { --c: var(--line); }
.dot { background: var(--c); }
.sec { font-weight: 600; margin: 18px 0 6px; color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .04em; }
.unit { padding: 3px 8px; border-left: 3px solid var(--c); margin: 2px 0; cursor: pointer; border-radius: 0 4px 4px 0; }
.unit:hover, .row:hover { background: var(--code-bg); }
.unit.code { font-family: ui-monospace, monospace; font-size: 12px; white-space: pre-wrap; }
.unit .ids { color: var(--muted); font-size: 11px; margin-left: 6px; }
.unit.sel, .row.sel { background: var(--code-bg); }
.row { display: flex; gap: 8px; align-items: baseline; padding: 3px 6px; cursor: pointer; border-radius: 4px; }
.row .id { font-family: ui-monospace, monospace; font-size: 12px; min-width: 52px; }
.row .t { flex: 1; }
.detail h2 { font-size: 16px; margin: 0 0 4px; }
.detail .status { display: inline-block; padding: 1px 8px; border-radius: 10px; color: #fff; background: var(--c); font-size: 12px; }
.detail blockquote { margin: 8px 0; padding: 6px 10px; border-left: 3px solid var(--line); color: var(--muted); }
.detail .lab { font-weight: 600; margin-top: 12px; }
.detail ul { padding-left: 18px; margin: 4px 0; }
.detail p { margin: 4px 0; }
table.grid { border-collapse: collapse; font-size: 11px; }
table.grid th, table.grid td { border: 1px solid var(--line); padding: 2px 4px; text-align: center; }
table.grid th.v { writing-mode: vertical-rl; transform: rotate(180deg); white-space: nowrap; font-weight: normal; }
table.grid td.l { text-align: left; white-space: nowrap; cursor: pointer; }
table.grid td.h { background: color-mix(in srgb, var(--accent) calc(var(--k) * 100%), transparent); }
.file { margin: 10px 0; }
.file h3 { font-size: 13px; margin: 0 0 4px; font-family: ui-monospace, monospace; }
.item { font-family: ui-monospace, monospace; font-size: 12px; padding: 1px 6px; }
.item.none { color: var(--muted); }
.anc { display: inline-block; min-width: 52px; font-size: 10px; color: var(--muted); }
.anc-u { color: var(--diverges); font-weight: 600; }
.item a { margin-left: 4px; }
.bar { height: 6px; background: var(--line); border-radius: 3px; overflow: hidden; margin: 2px 0 6px; }
.bar > div { height: 100%; background: var(--accent); }
input[type=search] { width: 100%; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--ink); font: inherit; margin-bottom: 8px; }
</style>
</head>
<body>
<header>
  <h1>Spec ↔ code traceability</h1>
  <span class="muted" id="meta"></span>
  <nav id="tabs"></nav>
</header>
<main><div id="left"></div><div id="right"><p class="muted">Select a spec sentence or a clause to see where it is implemented.</p></div></main>
<script>
const D = /*DATA*/null;
const byId = Object.fromEntries(D.entries.map(e => [e.id, e]));
const byAnchor = Object.fromEntries(D.entries.map(e => [e.anchor, e]));
const statusOf = id => (byId[id] && byId[id].status) || "none";
const esc = s => String(s).replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;"}[c]));
const left = document.getElementById("left"), right = document.getElementById("right");
const clauses = D.entries.filter(e => e.kind === "clause");
document.getElementById("meta").textContent = `${clauses.length} clauses · ${D.units.length} spec sentences` + (D.commit ? ` · commit ${D.commit}` : "");
function worst(ids) {
  const order = ["Diverges", "Missing", "Partial", "Deferred", "Process", "Implemented"];
  for (const s of order) if (ids.some(i => statusOf(i) === s)) return s;
  return "none";
}
function legend() {
  return `<div class="legend">${D.statuses.map(s => `<span class="st-${s}"><span class="dot"></span>${s}</span>`).join("")}<span class="st-none"><span class="dot"></span>not quoted</span></div>`;
}
function show(id) {
  const e = byId[id] || byAnchor[id];
  if (!e) return;
  document.querySelectorAll(".sel").forEach(x => x.classList.remove("sel"));
  document.querySelectorAll(`[data-id="${e.id}"]`).forEach(x => x.classList.add("sel"));
  let h = `<div class="detail st-${e.status || "none"}"><h2>${esc(e.id)} · ${esc(e.title)}</h2>`;
  if (e.status) h += `<span class="status">${e.status}</span> `;
  h += `<span class="muted">${esc(e.file)}:${e.line}</span>`;
  for (const q of e.quotes_html) h += `<blockquote>${q}</blockquote>`;
  for (const [k, v] of Object.entries(e.sections_html)) h += `<div class="lab">${esc(k)}</div>${v}`;
  h += `</div>`;
  right.innerHTML = h;
  right.scrollTop = 0;
}
right.addEventListener("click", ev => { const a = ev.target.closest("[data-goto]"); if (a) { ev.preventDefault(); show(a.dataset.goto); } });
const views = {
  Spec() {
    let h = legend(), sec = null;
    for (const u of D.units) {
      if (u.section !== sec) { sec = u.section; h += `<div class="sec">${esc(sec)}</div>`; }
      const st = u.entries.length ? worst(u.entries) : "none";
      h += `<div class="unit st-${st} ${u.kind}" data-units="${u.entries.join(" ")}" ${u.entries[0] ? `data-id="${u.entries[0]}"` : ""}>${u.item ? `<b>${esc(u.item)}</b> ` : ""}${esc(u.text)}<span class="ids">${u.entries.join(" ")}</span></div>`;
    }
    left.innerHTML = h;
    left.querySelectorAll(".unit").forEach(el => el.onclick = () => { const ids = el.dataset.units.split(" ").filter(Boolean); if (ids.length) show(ids[0]); });
  },
  Clauses() {
    let h = `<input type="search" placeholder="Filter clauses…" id="q">` + legend() + `<div id="list"></div>`;
    left.innerHTML = h;
    const draw = q => {
      let out = "", area = null;
      for (const e of D.entries) {
        const text = (e.id + " " + e.title + " " + e.quotes.join(" ")).toLowerCase();
        if (q && !text.includes(q)) continue;
        if (e.area !== area) { area = e.area; out += `<div class="sec">${esc(area)}</div>`; }
        out += `<div class="row st-${e.status || "none"}" data-id="${e.id}"><span class="dot"></span><span class="id">${esc(e.id)}</span><span class="t">${esc(e.title)}</span></div>`;
      }
      document.getElementById("list").innerHTML = out;
      document.querySelectorAll("#list .row").forEach(el => el.onclick = () => show(el.dataset.id));
    };
    document.getElementById("q").oninput = ev => draw(ev.target.value.toLowerCase());
    draw("");
  },
  Matrix() {
    const cols = D.grid.cols;
    let h = `<p class="muted">Rows are clauses, columns source files; shading is the number of cited lines.</p><div style="overflow:auto"><table class="grid"><tr><th></th>${cols.map(c => `<th class="v">${esc(c.replace(/^src\//, ""))}</th>`).join("")}</tr>`;
    for (const r of D.grid.rows) {
      h += `<tr><td class="l st-${r.status || "none"}" data-id="${r.id}"><span class="dot"></span>${esc(r.id)}</td>`;
      for (const c of cols) { const n = r.cells[c] || 0; h += n ? `<td class="h" style="--k:${Math.min(1, 0.15 + n / 60)}" title="${n} lines">${n}</td>` : `<td></td>`; }
      h += `</tr>`;
    }
    left.innerHTML = h + `</table></div>`;
    left.querySelectorAll("td.l").forEach(el => el.onclick = () => show(el.dataset.id));
  },
  Code() {
    let h = `<p class="muted">Every item in <code>src/</code> with the entries that cite it. The tag is its spec ancestry: <b>spec</b> (cited by a clause), <b>part</b> (partly cited), <b>support</b> (reached from cited code) or <b class="anc-u">none</b> (no clause leads to it; see <code>generated/ancestry.md</code>).</p>`;
    for (const [f, items] of Object.entries(D.code)) {
      const fns = items.filter(i => i.kind === "fn"), cited = fns.filter(i => i.entries.length);
      h += `<div class="file"><h3>${esc(f)} <span class="muted">${cited.length}/${fns.length} fn</span></h3><div class="bar"><div style="width:${fns.length ? 100 * cited.length / fns.length : 0}%"></div></div>`;
      const tag = {"specified": "spec", "partly specified": "part", "support": "support", "unspecified": "none"};
      for (const it of items) h += `<div class="item ${it.entries.length ? "" : "none"}"><span class="anc ${it.ancestry === "unspecified" ? "anc-u" : ""}">${tag[it.ancestry] || ""}</span>${esc(it.kind)} ${esc(it.name)} <span class="muted">${it.start}</span>${it.entries.map(id => `<a href="#" data-goto="${byId[id] ? byId[id].anchor : id}">${esc(id)}</a>`).join("")}</div>`;
      h += `</div>`;
    }
    left.innerHTML = h;
    left.querySelectorAll("[data-goto]").forEach(a => a.onclick = ev => { ev.preventDefault(); show(a.dataset.goto); });
  },
};
const tabs = document.getElementById("tabs");
let current = "Spec";
try { current = localStorage.getItem("spectrace-tab") || "Spec"; } catch (e) {}
if (!views[current]) current = "Spec";
function go(name) {
  current = name;
  try { localStorage.setItem("spectrace-tab", name); } catch (e) {}
  tabs.querySelectorAll("button").forEach(b => b.classList.toggle("on", b.textContent === name));
  views[name]();
}
for (const name of Object.keys(views)) { const b = document.createElement("button"); b.textContent = name; b.onclick = () => go(name); tabs.appendChild(b); }
go(current);
</script>
</body>
</html>
"""
