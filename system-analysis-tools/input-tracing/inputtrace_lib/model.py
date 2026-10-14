"""The input model of a Rust + Dioxus code base.

It finds **entry points**, the places where something outside the program
reaches it:

- `handler`: an rsx event attribute (`onclick: move |e| …`);
- `effect`: a `use_effect` / `use_memo` closure (it reruns when a signal it
  reads changes, so it is where edits turn into work);
- `env`: an environment variable read;
- `fs`: a file-system read;
- `dialog`: a native dialog (rfd) whose answer flows back in;
- `webview`: JavaScript evaluation or a custom protocol handler.

It also finds three kinds of **edges**, which a trace follows:

- `call`: a resolved call to a function of the crate;
- `send`: a body sends `Msg::X` on a channel; the edge leads to the
  `match` arm that receives it;
- `react`: a body writes a signal that an effect subscribes to (reads with
  `.read()` or by calling it, not with `.peek()`).

Resolution is heuristic: it uses receiver names (`app.` → `App`, `worker.0.`
→ `Worker`, `g.` → `Graph`, `module::f`) and falls back to crate-unique
method names. Ambiguous calls are dropped, not guessed.
"""
from __future__ import annotations

import os
import re
from dataclasses import dataclass, field

from . import rustsrc

# --------------------------------------------------------------------------
# configuration (receiver names → owner types); override from the CLI


@dataclass
class Config:
    # receiver identifier → impl type its methods resolve to
    receivers: dict = field(default_factory=lambda: {
        "app": "App",
        "g": "Graph",
        "graph": "Graph",
        "ctx": "Ctx",
        "engine": "Engine",
        "project": "Project",
        "img": "SourceImage",
    })
    # receivers ending in `.0` (newtype handles) resolve to this type
    newtype_inner: str = "Worker"
    # the struct whose fields are signals (`app.field.set(…)`)
    state_type: str = "App"
    state_receivers: tuple = ("app",)
    # the channel message enum
    msg_enum: str = "Msg"
    src_dirs: tuple = ("src",)


# method names never resolved by crate-uniqueness alone (signal and std
# names that collide with crate methods)
_FALLBACK_BLOCK = set("""
set write read peek take toggle with_mut get insert remove push pop new render size clone map iter len
contains extend join name id file level run apply after inverse f i b text choice color finish store load
lock send spawn from into unwrap ok err as_ref filter find is_some is_none default fmt hash eq cmp
collect trim is_empty parse to_string as_str chars split split_once rsplit_once starts_with ends_with
unwrap_or unwrap_or_else unwrap_or_default map_err and_then or_else for_each into_iter iter_mut values keys
copied cloned flatten zip enumerate rev sum min max clamp round abs floor ceil sqrt exp is_file is_dir exists
display to_path_buf file_name extension parent kind data dims bytes pixels rect w h x y
""".split())

_KEYWORDS = set("if match while for loop return let fn move async await in as where impl dyn mut ref else".split())

SIGNAL_WRITE = ("set", "write", "take", "toggle", "with_mut")
SIGNAL_READ = ("read", "cloned", "with")


@dataclass
class Fn:
    qual: str  # Type::name or name
    name: str
    owner: str | None  # impl self type
    trait: str | None
    src: rustsrc.Source
    item: rustsrc.Item
    component: bool = False

    @property
    def file(self) -> str:
        return self.src.path

    @property
    def line(self) -> int:
        return self.item.line

    def body(self) -> tuple[int, int]:
        return (self.item.body, self.item.stop + 1) if self.item.body >= 0 else (self.item.stop, self.item.stop)


@dataclass
class Entry:
    id: str  # file:line:kind[:detail]
    kind: str  # handler | effect | env | fs | dialog | webview
    file: str
    line: int
    end_line: int
    fn: str  # enclosing fn qual
    detail: str  # event name, env var, …
    element: dict = field(default_factory=dict)  # handler: name, class, title, type, label
    span: tuple = (0, 0)  # body offsets in the masked text
    via: str | None = None  # handler bound to a local closure
    memo: str | None = None  # use_memo binding name

    def describe(self) -> str:
        if self.kind == "handler":
            e = self.element
            bits = [e.get("name", "?")]
            for k in ("type", "class"):
                if e.get(k):
                    bits.append(f'{k}="{e[k]}"')
            lab = e.get("label") or e.get("title")
            s = f"{self.detail} on <{' '.join(bits)}>"
            if lab:
                s += f" “{lab}”"
            return s + (f" (via `{self.via}`)" if self.via else "")
        if self.kind == "effect":
            return f"{self.detail}" + (f" → `{self.memo}`" if self.memo else "")
        return self.detail


@dataclass
class Arm:
    variant: str
    file: str
    line: int
    fn: str
    src: rustsrc.Source
    span: tuple


@dataclass
class Index:
    root: str
    cfg: Config
    sources: dict  # path → Source
    fns: list
    by_qual: dict  # qual → [Fn]
    entries: list
    arms: dict  # Msg variant → [Arm]
    local_signals: dict  # fn qual → {name}
    memos: dict  # (fn qual, name) → Entry
    line_starts: dict  # path → [offsets]
    arm_patterns: dict = field(default_factory=dict)  # path → [(start, end)] of receiving arm patterns

    # ---- positions ------------------------------------------------------

    def line_of(self, path: str, off: int) -> int:
        ls = self.line_starts[path]
        lo, hi = 0, len(ls) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if ls[mid] <= off:
                lo = mid
            else:
                hi = mid - 1
        return lo + 1

    def enclosing_fn(self, path: str, off: int) -> Fn | None:
        best = None
        for f in self.fns:
            if f.file == path and f.item.start <= off <= f.item.stop:
                if best is None or f.item.start >= best.item.start:
                    best = f
        return best

    def lookup(self, key: str) -> list:
        """Candidates for `key` (Entry or Fn) from the first rule that gives
        any: exact entry id; `file:line` (entries whose span contains the
        line, innermost first; if none, the innermost fn whose span contains
        it; nothing else); exact fn qual; fn whose qual
        ends in `::key` or whose name is `key`; entry ids containing `key`.
        One candidate is a match; more are ambiguous."""
        hits = [e for e in self.entries if e.id == key]
        if hits:
            return hits
        m = re.fullmatch(r"(.+?):(\d+)", key)
        if m:
            path, line = m.group(1), int(m.group(2))
            hits = [e for e in self.entries if (e.file == path or e.file.endswith("/" + path)) and e.line <= line <= e.end_line]
            if not hits:
                # no entry there: the fn whose span contains the line (this is
                # how a fn with a duplicate name is chosen)
                hits = [f for f in self.fns if (f.file == path or f.file.endswith("/" + path)) and f.item.line <= line <= f.item.end]
                if len({f.file for f in hits}) == 1:
                    inner = min(f.item.end - f.item.line for f in hits)
                    hits = [f for f in hits if f.item.end - f.item.line == inner]
                return hits
            if len({e.file for e in hits}) == 1:
                # nested spans in one file: the innermost
                inner = min(e.end_line - e.line for e in hits)
                hits = [e for e in hits if e.end_line - e.line == inner]
            return hits
        hits = list(self.by_qual.get(key, []))
        if hits:
            return hits
        hits = [f for f in self.fns if f.qual.endswith("::" + key) or f.name == key]
        if hits:
            return hits
        return [e for e in self.entries if key in e.id]

    def entry(self, key: str) -> Entry | None:
        """The one entry `key` names (see `lookup`), else None."""
        hits = [h for h in self.lookup(key) if isinstance(h, Entry)]
        return hits[0] if len(hits) == 1 else None

    def fn(self, key: str) -> Fn | None:
        fs = self.by_qual.get(key)
        if fs:
            return fs[0]
        hits = [f for f in self.fns if f.qual.endswith("::" + key) or f.name == key]
        return hits[0] if len(hits) == 1 else None


# --------------------------------------------------------------------------
# bracket helpers on masked text

_OPEN = {"(": ")", "[": "]", "{": "}"}
_CLOSE = {")", "]", "}"}


def match_close(s: str, i: int) -> int:
    """Offset of the bracket closing the one at `i`."""
    depth = 0
    for k in range(i, len(s)):
        c = s[k]
        if c in _OPEN:
            depth += 1
        elif c in _CLOSE:
            depth -= 1
            if depth == 0:
                return k
    return len(s) - 1


def expr_end(s: str, i: int, stops: str = ",") -> int:
    """End of the expression starting at `i`: the first depth-0 `stops`
    character, or a closing bracket of an enclosing group."""
    depth = 0
    for k in range(i, len(s)):
        c = s[k]
        if c in _OPEN:
            depth += 1
        elif c in _CLOSE:
            if depth == 0:
                return k
            depth -= 1
        elif depth == 0 and c in stops:
            return k
    return len(s)


def open_before(s: str, i: int, lo: int = 0) -> int:
    """Offset of the innermost unmatched `{` before `i` (or -1)."""
    depth = 0
    for k in range(i - 1, lo - 1, -1):
        c = s[k]
        if c in _CLOSE:
            depth += 1
        elif c in _OPEN:
            if depth == 0:
                return k if c == "{" else open_before(s, k, lo)
            depth -= 1
    return -1


_STR = re.compile(r'"((?:[^"\\]|\\.)*)"')


def first_string(text: str, a: int, b: int) -> str | None:
    m = _STR.search(text, a, b)
    return m.group(1) if m else None


# --------------------------------------------------------------------------
# building the index


class MissingSourceDir(Exception):
    """A source directory (`root/DIR` for each `src_dirs` entry) does not exist."""


def build(root: str, cfg: Config | None = None) -> Index:
    cfg = cfg or Config()
    for d in cfg.src_dirs:
        if not os.path.isdir(os.path.join(root, d)):
            raise MissingSourceDir(os.path.join(root, d))
    sources: dict = {}
    for rel in rustsrc.rust_files(root, cfg.src_dirs):
        sources[rel] = rustsrc.load(root, rel)
    fns: list = []
    by_qual: dict = {}
    line_starts: dict = {}
    for path, src in sources.items():
        ls = [0]
        for k, c in enumerate(src.masked):
            if c == "\n":
                ls.append(k + 1)
        line_starts[path] = ls
        impls = [it for it in src.items if it.kind in ("impl", "trait")]
        for it in src.items:
            if it.kind != "fn":
                continue
            owner = trait = None
            for c in impls:
                if c.start < it.start and it.stop <= c.stop:
                    owner, trait = c.name, c.trait
            body = src.masked[it.body:it.stop] if it.body >= 0 else ""
            comp = any("component" in a for a in it.attrs) or ("rsx!" in body and "-> Element" in src.masked[it.start:it.body])
            f = Fn(it.qual, it.name, owner, trait, src, it, comp)
            fns.append(f)
            by_qual.setdefault(it.qual, []).append(f)
    idx = Index(root, cfg, sources, fns, by_qual, [], {}, {}, {}, line_starts)
    _local_signals(idx)
    _entries(idx)
    _arms(idx)
    return idx


def _local_signals(idx: Index) -> None:
    """Per fn: local signal name → signal id. `let x = use_signal(…)` gives
    `Fn.x`; a `x: Signal<…>` parameter is aliased to the same-named local
    signal of the one fn in the same file that declares it (the component
    that passes it down)."""
    declared: dict = {}
    for f in idx.fns:
        a, b = f.body()
        body = f.src.masked[a:b]
        # a list in source order, not a set: output must not depend on the hash seed
        names = list(dict.fromkeys(m.group(1) for m in re.finditer(r"\blet\s+(?:mut\s+)?(\w+)\s*=\s*use_(?:signal|memo)\b", body)))
        if names:
            idx.local_signals[f.qual] = {n: f"{f.qual}.{n}" for n in names}
            for n in names:
                declared.setdefault((f.file, n), []).append(f.qual)
    for f in idx.fns:
        header = f.src.masked[f.item.start:f.item.body] if f.item.body >= 0 else ""
        for pm in re.finditer(r"(?:mut\s+)?(\w+)\s*:\s*Signal\s*<", header):
            n = pm.group(1)
            owners = declared.get((f.file, n), [])
            sig = f"{owners[0]}.{n}" if len(owners) == 1 else f"{f.qual}.{n}"
            idx.local_signals.setdefault(f.qual, {})[n] = sig


def _component_of(idx: Index, path: str, off: int) -> Fn | None:
    f = idx.enclosing_fn(path, off)
    return f


def _element(src: rustsrc.Source, pos: int, lo: int) -> dict:
    """The rsx element a handler attribute at `pos` belongs to."""
    m, t = src.masked, src.text
    ob = open_before(m, pos, lo)
    if ob < 0:
        return {}
    head = m[max(lo, ob - 80):ob].rstrip()
    nm = re.search(r"([A-Za-z_][\w:]*)\s*$", head)
    el = {"name": nm.group(1) if nm else "?"}
    cb = match_close(m, ob)
    # depth-1 attributes `key: value` and the first child text literal
    k, depth = ob + 1, 0
    attr_spans = []
    while k < cb:
        c = m[k]
        if c in _OPEN:
            depth += 1
        elif c in _CLOSE:
            depth -= 1
        elif depth == 0:
            am = re.match(r"(r#)?([a-z_]\w*)\s*:(?!:)", m[k:k + 40])
            if am and (k == 0 or not (m[k - 1].isalnum() or m[k - 1] == "_")):
                key = am.group(2)
                vs = k + am.end()
                ve = expr_end(m, vs)
                attr_spans.append((k, ve))
                if key in ("class", "title", "type", "value", "accept"):
                    s = first_string(t, vs, ve)
                    if s is not None:
                        el[key] = s
                k = ve
                continue
            if t[k] == '"' and "label" not in el and not any(a <= k < b for a, b in attr_spans):
                s = first_string(t, k, cb)
                if s is not None:
                    el["label"] = s
        k += 1
    return el


_HANDLER = re.compile(r"(?<![\w.])(on[a-z]+)\s*:(?!:)")


def _entries(idx: Index) -> None:
    out = []
    for f in idx.fns:
        src = f.src
        a, b = f.body()
        m, t = src.masked, src.text
        if f.component or "rsx!" in m[a:b]:
            for hm in _HANDLER.finditer(m, a, b):
                # only inside an rsx! block
                if "rsx!" not in m[a:hm.start()]:
                    continue
                vs = hm.end()
                while vs < b and m[vs] in " \t\n":
                    vs += 1
                ve = expr_end(m, vs)
                via = None
                val = m[vs:ve].strip()
                span = (vs, ve)
                if re.fullmatch(r"[a-z_]\w*", val):
                    lm = None
                    for lm in re.finditer(rf"\blet\s+(?:mut\s+)?{val}\s*=", m[a:hm.start()]):
                        pass
                    if lm:
                        rs = a + lm.end()
                        span = (rs, expr_end(m, rs, ";"))
                        via = val
                    else:
                        # a tuple binding like `let (x, y, z) = (…)`
                        tm = None
                        for tm in re.finditer(rf"\blet\s+\(([^)]*\b{val}\b[^)]*)\)\s*=", m[a:hm.start()]):
                            pass
                        if tm:
                            rs = a + tm.end()
                            span = (rs, expr_end(m, rs, ";"))
                            via = val
                line = idx.line_of(src.path, hm.start())
                end_line = idx.line_of(src.path, span[1])
                if via:
                    end_line = idx.line_of(src.path, ve)
                el = _element(src, hm.start(), a)
                out.append(Entry(f"{src.path}:{line}:{hm.group(1)}", "handler", src.path, line, end_line, f.qual, hm.group(1), el, span, via))
        for em in re.finditer(r"\buse_(effect|memo)\s*\(", m[a:b]):
            p = a + em.start()
            po = a + em.end() - 1
            pc = match_close(m, po)
            line = idx.line_of(src.path, p)
            bm = re.search(r"\blet\s+(?:mut\s+)?(\w+)\s*=\s*$", m[max(a, p - 60):p])
            e = Entry(f"{src.path}:{line}:use_{em.group(1)}", "effect", src.path, line, idx.line_of(src.path, pc), f.qual,
                      f"use_{em.group(1)} in {f.qual}", {}, (po + 1, pc), None, bm.group(1) if bm else None)
            out.append(e)
            if e.memo:
                idx.memos[(f.qual, e.memo)] = e
        for vm in re.finditer(r"\benv::var(?:_os)?\s*\(", m[a:b]):
            p = a + vm.start()
            name = first_string(t, p, p + 80) or "?"
            line = idx.line_of(src.path, p)
            out.append(Entry(f"{src.path}:{line}:env:{name}", "env", src.path, line, line, f.qual, f"env {name}", {}, (p, p)))
        for fm in re.finditer(r"\b(fs::read(?:_to_string|_dir)?|File::open|ZipArchive::new|read_bytes)\s*\(", m[a:b]):
            p = a + fm.start()
            line = idx.line_of(src.path, p)
            out.append(Entry(f"{src.path}:{line}:fs:{fm.group(1)}", "fs", src.path, line, line, f.qual, f"{fm.group(1)} in {f.qual}", {}, (p, p)))
        for dm in re.finditer(r"\brfd::(Async\w*Dialog)\b", m[a:b]):
            p = a + dm.start()
            line = idx.line_of(src.path, p)
            out.append(Entry(f"{src.path}:{line}:dialog", "dialog", src.path, line, line, f.qual, f"{dm.group(1)} in {f.qual}", {}, (p, p)))
        for wm in re.finditer(r"\b(document::eval|use_asset_handler)\s*\(", m[a:b]):
            p = a + wm.start()
            line = idx.line_of(src.path, p)
            out.append(Entry(f"{src.path}:{line}:webview", "webview", src.path, line, line, f.qual, f"{wm.group(1)} in {f.qual}", {}, (p, p)))
    # nested fns (a component inside a component) would double-count
    seen, uniq = set(), []
    for e in sorted(out, key=lambda e: (e.file, e.line, e.id)):
        if e.id not in seen:
            seen.add(e.id)
            uniq.append(e)
    idx.entries = uniq


def _arms(idx: Index) -> None:
    me = idx.cfg.msg_enum
    for f in idx.fns:
        a, b = f.body()
        m = f.src.masked
        for mm in re.finditer(r"\bmatch\b", m[a:b]):
            # the scrutinee is any expression (`msg`, `rx.recv()`): the arms
            # open at the first depth-0 `{` after it
            ob, depth = -1, 0
            for j in range(a + mm.end(), b):
                c = m[j]
                if c in "([":
                    depth += 1
                elif c in ")]":
                    depth -= 1
                    if depth < 0:
                        break
                elif depth == 0 and c in ";}":
                    break
                elif depth == 0 and c == "{":
                    ob = j
                    break
            if ob < 0:
                continue
            cb = match_close(m, ob)
            k = ob + 1
            while k < cb:
                # every arm, whatever its shape (`Msg::X`, `Ok(Msg::X)`,
                # `Msg::A | Msg::B`, `_`): each `Msg::V` in its pattern is a receive
                arrow = m.find("=>", k, cb)
                if arrow < 0:
                    break
                variants = re.findall(rf"\b{me}::(\w+)", m[k:arrow])
                if variants:
                    idx.arm_patterns.setdefault(f.file, []).append((k, arrow))
                vs = arrow + 2
                while m[vs] in " \t\n":
                    vs += 1
                if m[vs] == "{":
                    ve = match_close(m, vs) + 1
                    if ve < cb and m[ve:].lstrip().startswith(","):
                        ve = m.index(",", ve) + 1
                else:
                    ve = expr_end(m, vs) + 1
                seen: set = set()
                for vm in re.finditer(rf"\b{me}::(\w+)", m[k:arrow]):
                    v = vm.group(1)
                    if v not in seen:
                        seen.add(v)
                        line = idx.line_of(f.file, k + vm.start(1))
                        idx.arms.setdefault(v, []).append(Arm(v, f.file, line, f.qual, f.src, (vs, ve)))
                k = ve


# --------------------------------------------------------------------------
# edges out of a body


@dataclass
class Edges:
    calls: list = field(default_factory=list)  # [(Fn, line)]
    sends: list = field(default_factory=list)  # [(variant, line)]
    writes: list = field(default_factory=list)  # [signal]
    reads: list = field(default_factory=list)  # [signal] (subscribing)
    spawns: list = field(default_factory=list)  # ["thread" | "async"]


_CALL = re.compile(r"(?<![\w!])([a-z_]\w*)\s*(?:::<[^>]*>)?\s*\(")


def _receiver(m: str, p: int) -> tuple[str | None, str | None]:
    """(kind, token) of what precedes the call name at `p`: ('method', recv)
    for `recv.name(`, ('path', module) for `module::name(`, else (None, None)."""
    pre = m[max(0, p - 120):p]
    mm = re.search(r"([\w]+)((?:\.0)?)(?:\.clone\(\))?\.\s*$", pre)
    if mm:
        return "method", mm.group(1) + mm.group(2)
    if re.search(r"\)\s*\.\s*$", pre) or re.search(r"\.\s*$", pre):
        return "method", None
    pm = re.search(r"(\w+)\s*::\s*$", pre)
    if pm:
        return "path", pm.group(1)
    return None, None


def _module_fns(idx: Index, module: str, name: str) -> list:
    out = []
    for f in idx.by_qual.get(name, []):
        stem = os.path.splitext(os.path.basename(f.file))[0]
        parent = os.path.basename(os.path.dirname(f.file))
        if stem == module or (stem == "mod" and parent == module):
            out.append(f)
    return out


def resolve(idx: Index, owner_fn: Fn | None, m: str, p: int, name: str) -> Fn | None:
    if name in _KEYWORDS:
        return None
    if re.match(r"\s*fn\s*$", m[max(0, p - 4):p]) or m[max(0, p - 3):p].endswith("fn "):
        return None
    kind, tok = _receiver(m, p)
    cfg = idx.cfg
    if kind == "method":
        if tok is None:
            owner = None
        elif tok.endswith(".0"):
            owner = cfg.newtype_inner
        elif tok == "self" and owner_fn is not None:
            owner = owner_fn.owner
        else:
            owner = cfg.receivers.get(tok)
        if owner:
            fs = idx.by_qual.get(f"{owner}::{name}")
            if fs:
                return fs[0]
            return None
        # signal ops on a field (`app.toast.set(`) are not calls
        if name in _FALLBACK_BLOCK:
            return None
        cands = [f for f in idx.fns if f.name == name and f.owner]
        return cands[0] if len(cands) == 1 else None
    if kind == "path":
        if tok == "Self":
            # `Self::name(`: the enclosing impl's type
            tok = owner_fn.owner if owner_fn is not None else None
            if not tok:
                return None
        if tok and tok[0].isupper():
            fs = idx.by_qual.get(f"{tok}::{name}")
            return fs[0] if fs else None
        fs = _module_fns(idx, tok, name)
        return fs[0] if len(fs) == 1 else None
    # bare call: same file first, then crate-unique free fn
    if owner_fn is not None:
        same = [f for f in idx.by_qual.get(name, []) if f.file == owner_fn.file and not f.owner]
        if same:
            return same[0]
    cands = [f for f in idx.by_qual.get(name, []) if not f.owner]
    return cands[0] if len(cands) == 1 else None


def _closure(idx: Index, ctx_fn: Fn | None, name: str, before: int) -> tuple | None:
    """Span of `let name = <closure or expression>;` in `ctx_fn` before
    offset `before` (the last such binding)."""
    if ctx_fn is None:
        return None
    a, _ = ctx_fn.body()
    m = ctx_fn.src.masked
    lm = None
    for lm in re.finditer(rf"\blet\s+(?:mut\s+)?{re.escape(name)}\s*=\s*(?=move\b|\|)", m[a:before]):
        pass
    if not lm:
        return None
    rs = a + lm.end()
    return (rs, expr_end(m, rs, ";"))


def edges(idx: Index, src: rustsrc.Source, span: tuple, ctx_fn: Fn | None, _inlined: frozenset = frozenset(),
          holes: tuple = ()) -> Edges:
    """Edges out of `span`, ignoring the sub-spans in `holes`."""
    a, b = span
    m = src.masked
    for ha, hb in holes:
        m = m[:ha] + re.sub(r"[^\n]", " ", m[ha:hb]) + m[hb:]
    body = m[a:b]
    e = Edges()
    seen = set()
    for cm in _CALL.finditer(m, a, b):
        name = cm.group(1)
        f = resolve(idx, ctx_fn, m, cm.start(1), name)
        if f is None and _receiver(m, cm.start(1))[0] is None and name not in _inlined:
            # a call to a local closure: its body's edges count as ours
            cl = _closure(idx, ctx_fn, name, cm.start())
            if cl is not None:
                sub = edges(idx, src, cl, ctx_fn, _inlined | {name})
                for g, line in sub.calls:
                    if g.qual not in seen:
                        seen.add(g.qual)
                        e.calls.append((g, line))
                e.sends += [s for s in sub.sends if s not in e.sends]
                e.writes += [w for w in sub.writes if w not in e.writes]
                e.reads += [r for r in sub.reads if r not in e.reads]
                e.spawns += sub.spawns
            continue
        if f is not None and f is not ctx_fn and f.qual not in seen:
            seen.add(f.qual)
            e.calls.append((f, idx.line_of(src.path, cm.start())))
    me = idx.cfg.msg_enum
    # every `Msg::X` is a send except the patterns of receiving arms
    pats = idx.arm_patterns.get(src.path, [])
    for sm in re.finditer(rf"\b{me}::(\w+)", body):
        if any(ps <= a + sm.start() < pe for ps, pe in pats):
            continue
        if sm.group(1) not in [s for s, _ in e.sends]:
            e.sends.append((sm.group(1), idx.line_of(src.path, a + sm.start())))
    st = idx.cfg.state_type
    recv = "|".join(idx.cfg.state_receivers + (("self",) if ctx_fn is not None and ctx_fn.owner == st else ()))
    for wm in re.finditer(rf"\b(?:{recv})\.(\w+)(?:\.clone\(\))?\.({'|'.join(SIGNAL_WRITE)})\s*\(", body):
        s = f"{st}.{wm.group(1)}"
        if s not in e.writes:
            e.writes.append(s)
    for rm in re.finditer(rf"\b(?:{recv})\.(\w+)\.({'|'.join(SIGNAL_READ)})\s*\(", body):
        s = f"{st}.{rm.group(1)}"
        if s not in e.reads:
            e.reads.append(s)
    if ctx_fn is not None:
        for name, sig in idx.local_signals.get(ctx_fn.qual, {}).items():
            if re.search(rf"(?<![\w.]){name}\.({'|'.join(SIGNAL_WRITE)})\s*\(", body) and sig not in e.writes:
                e.writes.append(sig)
            if re.search(rf"(?<![\w.]){name}(?:\s*\(\s*\)|\.({'|'.join(SIGNAL_READ)})\s*\()", body) and sig not in e.reads:
                e.reads.append(sig)
    if re.search(r"\b(?:thread::spawn|thread::Builder|run_background)\b", body):
        e.spawns.append("thread")
    if re.search(r"(?<![\w_])spawn\s*\(\s*async\b|\bspawn_local\s*\(", body):
        e.spawns.append("async")
    return e


def fn_edges(idx: Index, f: Fn) -> Edges:
    return edges(idx, f.src, f.body(), f)


def entry_edges(idx: Index, e: Entry) -> Edges:
    src = idx.sources[e.file]
    return edges(idx, src, e.span, idx.enclosing_fn(e.file, e.span[0]))


def arm_edges(idx: Index, arm: Arm) -> Edges:
    return edges(idx, arm.src, arm.span, idx.enclosing_fn(arm.file, arm.span[0]))


# --------------------------------------------------------------------------
# reactivity


def effect_reads(idx: Index, e: Entry, depth: int = 6) -> set:
    """Signals an effect subscribes to: its own reads, reads of the
    functions it calls (transitively) and of the memos it reads."""
    out: set = set()
    ed = entry_edges(idx, e)
    out.update(ed.reads)
    stack = [(f, 1) for f, _ in ed.calls]
    seen = set()
    while stack:
        f, d = stack.pop()
        if f.qual in seen or d > depth:
            continue
        seen.add(f.qual)
        fe = fn_edges(idx, f)
        out.update(fe.reads)
        stack.extend((g, d + 1) for g, _ in fe.calls)
    for s in list(out):
        owner, _, name = s.rpartition(".")
        memo = idx.memos.get((owner, name))
        if memo is not None and memo is not e:
            out |= effect_reads(idx, memo, depth)
    return out


def subscribers(idx: Index) -> dict:
    """signal → [effect Entry] subscribed to it."""
    subs: dict = {}
    for e in idx.entries:
        if e.kind == "effect":
            for s in effect_reads(idx, e):
                subs.setdefault(s, []).append(e)
    return subs


def writers(idx: Index) -> dict:
    """signal → [(where, line)] that write it directly."""
    out: dict = {}
    for f in idx.fns:
        # a write inside a handler or effect belongs to that entry, not to
        # the component that contains it
        a, b = f.body()
        holes = tuple(e.span for e in idx.entries
                      if e.kind in ("handler", "effect") and e.file == f.file and a <= e.span[0] and e.span[1] <= b)
        for s in edges(idx, f.src, (a, b), f, holes=holes).writes:
            out.setdefault(s, []).append((f.qual, f.file, f.line))
    for e in idx.entries:
        if e.kind in ("handler", "effect"):
            for s in entry_edges(idx, e).writes:
                out.setdefault(s, []).append((e.id, e.file, e.line))
    return out
