"""The code side: a small Rust reader. It masks comments and string
literals (keeping line structure), then finds items (fn, struct, enum,
trait, impl, const, static, mod) and their line spans by brace matching.
Good enough for traceability; it has no types and does not expand macros.
"""
from __future__ import annotations

import os
import re
from dataclasses import dataclass, field

ITEM = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|extern\s+\"C\")\s+)*"
    r"(fn|struct|enum|trait|impl|const|static|mod|type)\b\s*(.*)$"
)
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def mask(src: str) -> str:
    """Replaces the contents of comments, strings and char literals with
    spaces. Newlines are kept, so line numbers do not move."""
    out = list(src)
    i, n = 0, len(src)

    def blank(a: int, b: int):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(i, j)
            i = j
        elif c == "r" and re.match(r'r#*"', src[i:i + 10]) and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            m = re.match(r'r(#*)"', src[i:])
            close = '"' + m.group(1)
            j = src.find(close, i + len(m.group(0)))
            j = n if j < 0 else j + len(close)
            blank(i + 1, j - 1)
            i = j
        elif c == "b" and i + 1 < n and src[i + 1] == '"' and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            i += 1  # byte string: handled as a string next
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            blank(i + 1, j)
            i = j + 1
        elif c == "'":
            m = re.match(r"'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'])'", src[i:])
            if m:
                blank(i + 1, i + len(m.group(0)) - 1)
                i += len(m.group(0))
            else:
                i += 1  # a lifetime
        else:
            i += 1
    return "".join(out)


@dataclass
class Item:
    kind: str
    name: str          # "Graph::connect", "impl NodeKind for Clip", "SPLIT"
    file: str
    start: int         # 1-based, the signature line
    end: int
    doc: str = ""
    is_test: bool = False

    @property
    def short(self) -> str:
        return self.name.split("::")[-1]

    def to_json(self) -> dict:
        return {"kind": self.kind, "name": self.name, "file": self.file, "start": self.start, "end": self.end, "test": self.is_test}


@dataclass
class Source:
    path: str               # relative to root
    lines: list[str]
    masked: list[str]
    items: list[Item] = field(default_factory=list)

    def item_at(self, line: int) -> Item | None:
        """Innermost item of any kind containing `line` (the smallest span):
        the fn, or the impl or module around it when no fn contains the line."""
        best = None
        for it in self.items:
            if it.start <= line <= it.end and (best is None or it.end - it.start < best.end - best.start):
                best = it
        return best

    def fn_names(self) -> set[str]:
        return {it.short for it in self.items if it.kind == "fn"}


def _block_end(masked: list[str], start: int) -> int:
    """Last line (1-based) of the item starting at `start`: the matching
    brace of its first `{`, or the first `;` at depth 0."""
    depth = 0
    nest = 0  # ( and [ depth: a `;` inside `[f32; 3]` ends nothing
    opened = False
    for ln in range(start - 1, len(masked)):
        for ch in masked[ln]:
            if ch in "([":
                nest += 1
            elif ch in ")]":
                nest -= 1
            elif ch == "{":
                depth += 1
                opened = True
            elif ch == "}":
                depth -= 1
                if opened and depth == 0:
                    return ln + 1
            elif ch == ";" and not opened and depth == 0 and nest <= 0:
                return ln + 1
    return len(masked)


def _impl_name(rest: str) -> str:
    rest = re.sub(r"^<[^>]*>\s*", "", rest).split("{")[0].strip()
    m = re.match(r"(.+?)\s+for\s+(.+)$", rest)
    if m:
        return f"impl {m.group(1).strip()} for {m.group(2).strip()}"
    return f"impl {rest}"


def _impl_self(rest: str) -> str:
    rest = re.sub(r"^<[^>]*>\s*", "", rest).split("{")[0].strip()
    m = re.match(r".+?\s+for\s+(.+)$", rest)
    target = m.group(1) if m else rest
    return re.sub(r"<.*", "", target).strip().split("::")[-1]


def parse(path: str, text: str) -> Source:
    lines = text.splitlines()
    masked = mask(text).splitlines()
    masked += [""] * (len(lines) - len(masked))
    src = Source(path, lines, masked)
    impls: list[tuple[int, int, str]] = []
    tests: list[tuple[int, int]] = []
    for i, ml in enumerate(masked):
        m = ITEM.match(ml)
        if not m:
            continue
        kind, rest = m.group(1), m.group(2)
        if kind == "impl" and not re.match(r"^[\s<A-Za-z_]", rest or " "):
            continue
        start = i + 1
        end = _block_end(masked, start)
        if kind == "impl":
            name = _impl_name(rest)
            impls.append((start, end, _impl_self(rest)))
        else:
            nm = IDENT.match(rest)
            if not nm:
                continue
            name = nm.group(0)
            if kind == "mod" and name == "tests":
                tests.append((start, end))
        # doc comments directly above
        doc = []
        k = i - 1
        while k >= 0 and (lines[k].strip().startswith("///") or lines[k].strip().startswith("#[")):
            if lines[k].strip().startswith("///"):
                doc.append(lines[k].strip()[3:].strip())
            k -= 1
        is_test = any(lines[j].strip().startswith(("#[test]", "#[tokio::test")) for j in range(max(0, i - 3), i))
        src.items.append(Item(kind, name, path, start, end, " ".join(reversed(doc)), is_test))
    for it in src.items:
        if it.kind == "impl":
            continue
        owner = [s for (a, b, s) in impls if a < it.start <= b]
        if owner and it.kind in ("fn", "const", "type"):
            it.name = f"{owner[-1]}::{it.name}"
        if any(a <= it.start <= b for a, b in tests):
            it.is_test = True
    return src


class Tree:
    """All sources under the given directories, read lazily."""

    def __init__(self, root: str, dirs=("src", "tests")):
        self.root = root
        self.dirs = dirs
        self._cache: dict[str, Source] = {}

    def files(self) -> list[str]:
        out = []
        for d in self.dirs:
            base = os.path.join(self.root, d)
            for dp, _, fns in os.walk(base):
                for f in fns:
                    if f.endswith(".rs"):
                        out.append(os.path.relpath(os.path.join(dp, f), self.root))
        return sorted(out)

    def get(self, rel: str) -> Source | None:
        rel = os.path.normpath(rel)
        if rel in self._cache:
            return self._cache[rel]
        path = os.path.join(self.root, rel)
        if not os.path.isfile(path):
            return None
        with open(path, encoding="utf-8", errors="replace") as f:
            text = f.read()
        if rel.endswith(".rs"):
            src = parse(rel, text)
        else:
            lines = text.splitlines()
            src = Source(rel, lines, lines)
        self._cache[rel] = src
        return src


def identifier_tokens(name: str) -> list[str]:
    """`App::add_node` -> ["App", "add_node"]; `s.left` -> ["s", "left"]."""
    return [t for t in re.split(r"::|\.|\(|\)|\s|,|<|>|&|!|=|:|\[|\]|\"|'", name) if t and IDENT.fullmatch(t)]
