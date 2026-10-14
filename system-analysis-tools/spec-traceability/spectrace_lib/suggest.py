"""Candidate code for a piece of spec text: ranks source items by the
spec words they share (in their names, doc comments, string literals and
identifiers), weighted by how rare each word is across items. A reading
list for building an entry, not an answer.
"""
from __future__ import annotations

import math
import re
from collections import Counter

from . import rust

STOP = set("""a an and are as at be been being but by can could do does each either every for from has have if in into is it
its may must no nor not of on once or other our over should so some such than that the their them then there these they
this those through to under until up upon was we were what when where whether which while who will with within would
without you your also any both just more most only own same too very via one two new set get use used using make makes""".split())


def stem(w: str) -> str:
    w = w.lower()
    for suf in ("ization", "ations", "ation", "ings", "ing", "ies", "ied", "ers", "er", "ed", "es", "s"):
        if len(w) > len(suf) + 2 and w.endswith(suf):
            return w[: -len(suf)] + ("y" if suf in ("ies", "ied") else "")
    return w


def words(text: str) -> list[str]:
    # split identifiers: snake_case and CamelCase
    text = re.sub(r"([a-z])([A-Z])", r"\1 \2", text)
    out = []
    for w in re.findall(r"[A-Za-z][A-Za-z]+", text.replace("_", " ")):
        if w.lower() in STOP or len(w) < 3:
            continue
        out.append(stem(w))
    return out


def _item_terms(src: rust.Source, it: rust.Item) -> Counter:
    c = Counter()
    for w in words(it.name):
        c[w] += 4
    for w in words(it.doc):
        c[w] += 3
    body = src.lines[it.start - 1:it.end]
    strings = re.findall(r'"([^"\\]{3,})"', "\n".join(body))
    for s in strings:
        for w in words(s):
            c[w] += 2
    masked = "\n".join(src.masked[it.start - 1:it.end])
    for w in words(" ".join(rust.IDENT.findall(masked))):
        c[w] += 1
    comments = [l.strip()[2:] for l in body if l.strip().startswith("//")]
    for w in words(" ".join(comments)):
        c[w] += 2
    return c


def rank(text: str, tree: rust.Tree, top: int = 15, dirs=("src",)) -> list[dict]:
    items = []
    for f in tree.files():
        if not f.startswith(tuple(d + "/" for d in dirs)):
            continue
        src = tree.get(f)
        for it in src.items:
            if it.is_test or it.kind in ("impl", "mod", "struct", "enum", "type") and it.end - it.start > 200:
                continue
            items.append((src, it, _item_terms(src, it)))
    df = Counter()
    for _, _, terms in items:
        df.update(set(terms))
    n = len(items)
    query = Counter(words(text))
    scored = []
    for src, it, terms in items:
        s = 0.0
        hits = []
        for w in query:
            if w in terms:
                idf = math.log((n + 1) / (df[w] + 0.5))
                s += idf * math.log1p(terms[w])
                hits.append(w)
        if s:
            # prefer specific items to whole files' worth of code
            s /= 1 + math.log1p(max(1, it.end - it.start) / 40)
            scored.append({"score": round(s, 2), "item": it.name, "kind": it.kind, "file": it.file, "start": it.start, "end": it.end, "matched": hits})
    scored.sort(key=lambda d: -d["score"])
    return scored[:top]
