"""A small, dependency-free reader for Rust source.

It is not a parser. It does three things well enough for invariant hunting:

- split a file into code, comments and string literals (`lex`), producing a
  *masked* copy of the code in which comments and strings are blanked out but
  every character keeps its line and column;
- find items (fn, impl, trait, mod, struct, enum) and their brace-delimited
  bodies, so a line can be attributed to `Type::method` (`items`);
- list `#[test]` functions (`tests`).
"""
from __future__ import annotations

import os
import re
from dataclasses import dataclass, field


@dataclass
class Comment:
    line: int  # 1-based line of the comment start
    text: str  # without the comment markers
    doc: bool  # `///`, `//!`, `/** */`


@dataclass
class StrLit:
    line: int
    end_line: int
    text: str
    raw: bool


@dataclass
class Item:
    kind: str  # fn | impl | trait | mod | struct | enum
    name: str  # for impl: the self type
    qual: str  # Type::method, Trait, mod path not included
    line: int  # header line
    end: int  # last line of the body (== line for bodiless items)
    trait: str | None = None  # impl Trait for Type
    derives: list[str] = field(default_factory=list)
    attrs: list[str] = field(default_factory=list)
    doc: str = ""
    body: tuple[int, int] = (0, 0)  # (line, column) of the `{` or `;` that ends the header


@dataclass
class Source:
    path: str  # relative to the root
    text: str
    lines: list[str]
    masked: str
    masked_lines: list[str]
    comments: list[Comment]
    strings: list[StrLit]
    items: list[Item]

    def enclosing(self, line: int, kinds=("fn",)) -> Item | None:
        """Innermost item of one of `kinds` whose span contains `line`."""
        best = None
        for it in self.items:
            if it.kind in kinds and it.line <= line <= it.end:
                if best is None or it.line >= best.line:
                    best = it
        return best

    def where(self, line: int) -> str:
        it = self.enclosing(line, ("fn",)) or self.enclosing(line, ("impl", "trait", "mod", "struct", "enum"))
        return it.qual if it else ""


def lex(text: str):
    """Returns (masked, comments, strings)."""
    out = list(text)
    comments: list[Comment] = []
    strings: list[StrLit] = []
    i, n, line = 0, len(text), 1

    def blank(a, b):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            body = text[i + 2 : j]
            doc = body.startswith("/") and not body.startswith("//") or body.startswith("!")
            comments.append(Comment(line, body[1:] if doc else body, doc))
            blank(i, j)
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            body = text[i + 2 : j - 2]
            doc = body.startswith("*") or body.startswith("!")
            comments.append(Comment(line, body[1:] if doc else body, doc))
            blank(i, j)
            line += text.count("\n", i, j)
            i = j
        elif c in "rb" and (m := re.match(r"(?:br|r)(#*)\"", text[i:i + 70])) and not _ident_before(text, i):
            hashes = m.group(1)
            start = i + m.end()
            close = '"' + hashes
            j = text.find(close, start)
            j = n if j < 0 else j
            strings.append(StrLit(line, line + text.count("\n", i, j), text[start:j], True))
            blank(i, j + len(close))
            line += text.count("\n", i, j)
            i = j + len(close)
        elif c == '"' or (c == "b" and text.startswith('b"', i) and not _ident_before(text, i)):
            start = i + (2 if c == "b" else 1)
            j = start
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            strings.append(StrLit(line, line + text.count("\n", i, j), text[start:j], False))
            blank(i, j + 1)
            line += text.count("\n", i, j)
            i = j + 1
        elif c == "'":
            # char literal ('a', '\n', '\u{1F}') or a lifetime ('a, 'static)
            m = re.match(r"'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])'", text[i:i + 12])
            if m:
                blank(i, i + m.end())
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return "".join(out), comments, strings


def _ident_before(text: str, i: int) -> bool:
    return i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_")


_HEADER = re.compile(
    r"\b(?:(?:pub(?:\([^)]*\))?|const|async|unsafe|extern(?:\s+\"[^\"]*\")?|default)\s+)*"
    r"(fn|impl|trait|mod|struct|enum)\b"
)


def _match_brace(masked: str, i: int) -> int:
    depth = 0
    n = len(masked)
    while i < n:
        c = masked[i]
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return n - 1


def _strip_generics(s: str) -> str:
    out, depth = [], 0
    for c in s:
        if c == "<":
            depth += 1
        elif c == ">":
            depth -= 1
        elif depth == 0:
            out.append(c)
    return "".join(out)


def items(masked: str, text: str) -> list[Item]:
    line_starts = [0]
    for k, c in enumerate(masked):
        if c == "\n":
            line_starts.append(k + 1)

    def line_of(pos: int) -> int:
        lo, hi = 0, len(line_starts) - 1
        while lo < hi:
            mid = (lo + hi + 1) // 2
            if line_starts[mid] <= pos:
                lo = mid
            else:
                hi = mid - 1
        return lo + 1

    raw_lines = text.split("\n")
    found: list[tuple[int, int, Item]] = []  # (start, end, item)
    for m in _HEADER.finditer(masked):
        kind = m.group(1)
        # an item header starts a statement: `impl Trait` in a type position,
        # `fn(A) -> B` pointer types and the like are not items
        prev = masked[:m.start()].rstrip()
        if prev and prev[-1] not in "{};]":
            continue
        if kind == "fn" and not re.match(r"\s+[A-Za-z_]", masked[m.end():]):
            continue
        # header ends at the first `{` or `;` at bracket depth 0
        j, depth = m.end(), 0
        while j < len(masked):
            c = masked[j]
            if c in "([<" and not (c == "<" and masked[j - 1] == "-"):
                depth += 1
            elif c in ")]>" and not (c == ">" and masked[j - 1] in "-="):
                depth -= 1
            elif depth <= 0 and c in "{;":
                break
            j += 1
        if j >= len(masked):
            continue
        header = masked[m.end():j]
        if masked[j] == "{":
            end = _match_brace(masked, j)
        else:
            end = j
        start_line = line_of(m.start())
        it = Item(kind, "", "", start_line, line_of(end))
        it.body = (line_of(j), j - line_starts[line_of(j) - 1])
        if kind in ("fn", "trait", "mod", "struct", "enum"):
            nm = re.match(r"\s*([A-Za-z_]\w*)", header)
            if not nm:
                continue
            it.name = nm.group(1)
        else:  # impl
            h = _strip_generics(header)
            h = re.sub(r"\bwhere\b.*", "", h, flags=re.S)
            if " for " in f" {h} ":
                tr, ty = re.split(r"\bfor\b", h, maxsplit=1)
                it.trait = tr.strip().split("::")[-1].strip() or None
            else:
                ty = h
            ty = ty.strip().lstrip("&").strip()
            ty = re.sub(r"^(?:dyn|mut)\s+", "", ty)
            it.name = (ty.split("::")[-1].split()[0] if ty.split() else "?")
        # attributes and doc comments directly above the header
        k = start_line - 2
        attrs, docs = [], []
        while k >= 0:
            s = raw_lines[k].strip()
            if s.startswith("#["):
                attrs.append(s)
            elif s.startswith("///") or s.startswith("//!"):
                docs.append(s[3:].strip())
            elif s.startswith("//"):
                pass
            elif s == "" or not (s.startswith("#") or s.startswith("//")):
                # multi-line attributes: `)]` closing a `#[derive(` opened above
                if s.endswith(")]") or s.endswith(","):
                    attrs.append(s)
                    k -= 1
                    continue
                break
            k -= 1
        it.attrs = list(reversed(attrs))
        it.doc = " ".join(reversed(docs))
        for a in it.attrs:
            dm = re.search(r"derive\(([^)]*)\)", a)
            if dm:
                it.derives += [d.strip().split("::")[-1] for d in dm.group(1).split(",") if d.strip()]
        found.append((m.start(), end, it))

    # qualify: a fn inside an impl/trait becomes Type::fn
    containers = [(s, e, it) for s, e, it in found if it.kind in ("impl", "trait")]
    for s, e, it in found:
        it.qual = it.name
        if it.kind == "fn":
            owner = None
            for cs, ce, c in containers:
                if cs < s and e <= ce and (owner is None or cs > owner[0]):
                    owner = (cs, c)
            if owner:
                it.qual = f"{owner[1].name}::{it.name}"
    return [it for _, _, it in sorted(found, key=lambda t: t[0])]


def load(root: str, rel: str) -> Source:
    with open(os.path.join(root, rel), encoding="utf-8") as f:
        text = f.read()
    masked, comments, strings = lex(text)
    return Source(rel, text, text.split("\n"), masked, masked.split("\n"), comments, strings, items(masked, text))


def rust_files(root: str, dirs=("src", "tests")) -> list[str]:
    out = []
    for d in dirs:
        base = os.path.join(root, d)
        for dp, dn, fn in os.walk(base):
            dn[:] = sorted(x for x in dn if not x.startswith(".") and x != "target")
            for f in sorted(fn):
                if f.endswith(".rs"):
                    out.append(os.path.relpath(os.path.join(dp, f), root))
    return out


def tests(src: Source) -> list[Item]:
    """Functions carrying a #[test]-like attribute."""
    return [it for it in src.items if it.kind == "fn" and any(re.match(r"#\[(?:\w+::)*test\b", a) for a in it.attrs)]
