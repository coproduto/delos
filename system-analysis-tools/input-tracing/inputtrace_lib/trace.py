"""Walks the edges from an entry point and prints the path an input takes.

Each line is one hop:

    call  Type::fn                 a direct call
    send  Msg::X → recv in fn      a message on a channel, and the arm that takes it
    react App.field → effect       a signal write that reruns an effect

A function is expanded once per trace; later mentions say `(above)`.
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field

from . import model


@dataclass
class Options:
    depth: int = 6
    react: bool = True
    stop: str | None = None  # regex: quals not to expand
    collapse: tuple = ()  # quals printed but never expanded (e.g. engine internals)


@dataclass
class Node:
    label: str
    where: str
    notes: list = field(default_factory=list)
    children: list = field(default_factory=list)


def _notes(ed: model.Edges) -> list:
    out = []
    if ed.writes:
        out.append("writes " + ", ".join(ed.writes))
    if ed.spawns:
        out.append("hands off to " + " + ".join({"thread": "a background thread", "async": "an async task"}[s] for s in dict.fromkeys(ed.spawns)))
    return out


def build(idx: model.Index, root: model.Entry | model.Fn, opts: Options | None = None) -> Node:
    opts = opts or Options()
    subs = model.subscribers(idx) if opts.react else {}
    expanded: set = set()
    stop = re.compile(opts.stop) if opts.stop else None

    def kids(ed: model.Edges, depth: int, self_effect=None) -> list:
        out = []
        for f, line in ed.calls:
            out.append(fn_node(f, depth + 1, "call"))
        for variant, line in ed.sends:
            for arm in idx.arms.get(variant, []):
                key = f"arm:{arm.file}:{arm.line}"
                n = Node(f"send  {idx.cfg.msg_enum}::{variant} → received in {arm.fn}", f"{arm.file}:{arm.line}")
                if key in expanded:
                    n.label += "  (above)"
                elif depth + 1 >= opts.depth:
                    n.label += "  …"
                else:
                    expanded.add(key)
                    ae = model.arm_edges(idx, arm)
                    n.notes = _notes(ae)
                    n.children = kids(ae, depth + 1)
                out.append(n)
        if opts.react:
            for s in ed.writes:
                for eff in subs.get(s, []):
                    if eff is self_effect:
                        continue
                    out.append(effect_node(eff, s, depth + 1))
        return out

    def effect_node(eff: model.Entry, signal: str, depth: int) -> Node:
        n = Node(f"react {signal} → {eff.describe()}", f"{eff.file}:{eff.line}")
        key = f"effect:{eff.id}"
        if key in expanded:
            n.label += "  (above)"
            return n
        if depth >= opts.depth:
            n.label += "  …"
            return n
        expanded.add(key)
        ed = model.entry_edges(idx, eff)
        n.notes = _notes(ed)
        n.children = kids(ed, depth, eff)
        return n

    def fn_node(f: model.Fn, depth: int, how: str) -> Node:
        n = Node(f"{how}  {f.qual}", f"{f.file}:{f.line}")
        if f.qual in expanded:
            n.label += "  (above)"
            return n
        if f.qual in opts.collapse or (stop and stop.search(f.qual)):
            n.label += "  [not expanded]"
            return n
        if depth >= opts.depth:
            n.label += "  …"
            return n
        expanded.add(f.qual)
        ed = model.fn_edges(idx, f)
        n.notes = _notes(ed)
        n.children = kids(ed, depth)
        return n

    if isinstance(root, model.Entry):
        ed = model.entry_edges(idx, root)
        n = Node(f"{root.kind}  {root.describe()}  in {root.fn}", f"{root.file}:{root.line}", _notes(ed))
        n.children = kids(ed, 0, root if root.kind == "effect" else None)
        return n
    expanded.add(root.qual)
    ed = model.fn_edges(idx, root)
    n = Node(f"fn  {root.qual}", f"{root.file}:{root.line}", _notes(ed))
    n.children = kids(ed, 0)
    return n


def render(n: Node) -> str:
    lines = [f"{n.label}   [{n.where}]"]
    for note in n.notes:
        lines.append(f"    · {note}")

    def walk(node: Node, prefix: str) -> None:
        for i, c in enumerate(node.children):
            last = i == len(node.children) - 1
            lines.append(f"{prefix}{'└─ ' if last else '├─ '}{c.label}   [{c.where}]")
            ext = prefix + ("   " if last else "│  ")
            for note in c.notes:
                lines.append(f"{ext}    · {note}")
            walk(c, ext)

    walk(n, "")
    return "\n".join(lines)


def to_json(n: Node) -> dict:
    return {"label": n.label, "where": n.where, "notes": n.notes, "children": [to_json(c) for c in n.children]}
