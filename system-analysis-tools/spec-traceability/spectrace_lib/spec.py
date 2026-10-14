"""The specification side: split a Markdown spec into *units* (sentences,
or whole code blocks) and match quoted fragments against it.

Matching works on a normalised copy of the spec: list markers, item
numbers and `**` emphasis are removed and whitespace is collapsed. A
quote is a fragment of that text; `…` in a quote elides text, and the
pieces must then appear in order. Each unit records the character span it
occupies in the normalised text, so quotes can be mapped back to units.
"""
from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field

LIST_MARKER = re.compile(r"^\s*(?:[-*+]\s+(?:(\d+(?:\.\d+)*)\.\s+)?|(\d+(?:\.\d+)*)\.\s+)")
# Sentence boundary: end punctuation, space, then something that starts a
# sentence. Abbreviations are protected first.
SENTENCE_END = re.compile(r"(?<=[.!?])\s+(?=[A-Z(`*\"“])")
ABBREVIATIONS = ("e.g.", "i.e.", "etc.", "vs.", "cf.")


def normalise(text: str) -> str:
    text = text.replace("**", "")
    return re.sub(r"\s+", " ", text).strip()


@dataclass
class Unit:
    index: int
    line: int          # first line in the spec (1-based)
    end_line: int
    section: str       # "Basic functionalities", "Sessions … / Project structure"
    item: str          # spec numbering such as "3.1.1" when the unit sits in one
    kind: str          # "text" or "code"
    text: str          # normalised
    start: int = 0     # span in Spec.flat
    end: int = 0

    @property
    def digest(self) -> str:
        return hashlib.sha1(self.text.encode()).hexdigest()[:12]

    def to_json(self) -> dict:
        return {
            "index": self.index, "line": self.line, "end_line": self.end_line, "section": self.section,
            "item": self.item, "kind": self.kind, "text": self.text, "digest": self.digest,
        }


@dataclass
class Spec:
    path: str
    units: list[Unit] = field(default_factory=list)
    flat: str = ""

    def find(self, quote: str) -> tuple[int, int] | None:
        """Span of `quote` in the normalised spec, or None. Pieces split by
        `…` must occur in order; the span runs from the first to the last."""
        pieces = [normalise(p) for p in quote.split("…")]
        pieces = [p for p in pieces if p]
        if not pieces:
            return None
        start = self.flat.find(pieces[0])
        while start >= 0:
            pos = start + len(pieces[0])
            ok = True
            for p in pieces[1:]:
                k = self.flat.find(p, pos)
                if k < 0:
                    ok = False
                    break
                pos = k + len(p)
            if ok:
                return start, pos
            start = self.flat.find(pieces[0], start + 1)
        return None

    def units_in(self, span: tuple[int, int]) -> list[Unit]:
        a, b = span
        return [u for u in self.units if u.start < b and a < u.end]

    def unit_at_line(self, line: int) -> list[Unit]:
        return [u for u in self.units if u.line <= line <= u.end_line]


def split_sentences(text: str) -> list[str]:
    """Splits on sentence ends, never inside a `code span`."""
    spans: list[str] = []

    def hide(m: re.Match) -> str:
        spans.append(m.group(0))
        return f"\x01{len(spans) - 1}\x01"

    guarded = re.sub(r"`[^`]*`", hide, text)
    for i, a in enumerate(ABBREVIATIONS):
        guarded = guarded.replace(a, f"\x00{i}\x00")
    parts = SENTENCE_END.split(guarded)
    out = []
    for p in parts:
        for i, a in enumerate(ABBREVIATIONS):
            p = p.replace(f"\x00{i}\x00", a)
        p = re.sub(r"\x01(\d+)\x01", lambda m: spans[int(m.group(1))], p)
        if p.strip():
            out.append(p.strip())
    return out


def parse(text: str, path: str = "SPEC.md") -> Spec:
    lines = text.splitlines()
    spec = Spec(path)
    headings: list[tuple[int, str]] = []
    blocks: list[tuple[int, int, str, str, str]] = []  # (line, end, section, item, text|code)
    cur: list[str] = []
    cur_start = 0
    cur_item = ""
    in_code = False
    code: list[str] = []
    code_start = 0

    def section() -> str:
        return " / ".join(h for _, h in headings[1:]) if len(headings) > 1 else (headings[0][1] if headings else "")

    def flush(end: int):
        nonlocal cur, cur_item
        if cur:
            blocks.append((cur_start, end, section(), cur_item, "text", " ".join(cur)))
        cur = []
        cur_item = ""

    for n, raw in enumerate(lines, 1):
        if raw.strip().startswith("```"):
            if in_code:
                blocks.append((code_start, n, section(), "", "code", "\n".join(code)))
                in_code = False
            else:
                flush(n - 1)
                in_code, code, code_start = True, [], n
            continue
        if in_code:
            code.append(raw)
            continue
        m = re.match(r"^(#+)\s+(.*)$", raw)
        if m:
            flush(n - 1)
            level = len(m.group(1))
            headings[:] = [h for h in headings if h[0] < level]
            headings.append((level, m.group(2).strip()))
            continue
        if not raw.strip():
            flush(n - 1)
            continue
        marker = LIST_MARKER.match(raw)
        if marker:
            flush(n - 1)
            cur_item = marker.group(1) or marker.group(2) or ""
            cur_start = n
            cur = [raw[marker.end():]]
        else:
            if not cur:
                cur_start = n
            cur.append(raw.strip())
    flush(len(lines))

    flat_parts: list[str] = []
    pos = 0
    idx = 0
    for line, end, sec, item, kind, body in blocks:
        pieces = [normalise(body)] if kind == "code" else [normalise(s) for s in split_sentences(normalise(body))]
        for p in pieces:
            if not p:
                continue
            if flat_parts:
                pos += 1  # the joining space
            u = Unit(idx, line, end, sec, item, kind, p, pos, pos + len(p))
            spec.units.append(u)
            flat_parts.append(p)
            pos += len(p)
            idx += 1
    spec.flat = " ".join(flat_parts)
    return spec


def load(path: str) -> Spec:
    with open(path, encoding="utf-8") as f:
        return parse(f.read(), path)
