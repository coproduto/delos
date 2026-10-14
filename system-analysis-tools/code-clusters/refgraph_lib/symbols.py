"""SCIP symbol parsing and mapping of symbols to graph items.

A SCIP symbol is `<scheme> <manager> <package> <version> <descriptors>`
(spaces inside a field are escaped by doubling them), or `local <id>`.
Descriptors are `name/` (namespace), `name#` (type), `name.` (term),
`name().` (method), `[name]` (type parameter), `(name)` (parameter),
`name:` (meta) and `name!` (macro).

An *item* is the unit refgraph puts in the graph: a free function, a type
(fields, variants and nested members folded in), a constant/static, a macro,
or a method of a type (`Type::method`).
"""
import re

_SIMPLE = re.compile(r"[A-Za-z0-9_+\-$]+")


def split_symbol(sym):
    """Return ((manager, name, version), descriptor string), or None for locals."""
    if sym.startswith("local "):
        return None
    fields, cur, i = [], [], 0
    while i < len(sym) and len(fields) < 4:
        c = sym[i]
        if c == " ":
            if i + 1 < len(sym) and sym[i + 1] == " ":
                cur.append(" ")
                i += 2
                continue
            fields.append("".join(cur))
            cur = []
        else:
            cur.append(c)
        i += 1
    if len(fields) < 4:
        return None
    _scheme, manager, name, version = fields
    return (manager, name, version), sym[i:]


def _name(d, i):
    """Read a (possibly backtick-escaped) name at d[i]; return (name, next index)."""
    if i < len(d) and d[i] == "`":
        out, j = [], i + 1
        while j < len(d):
            if d[j] == "`":
                if j + 1 < len(d) and d[j + 1] == "`":
                    out.append("`")
                    j += 2
                    continue
                return "".join(out), j + 1
            out.append(d[j])
            j += 1
        raise ValueError(f"unterminated name in {d!r}")
    m = _SIMPLE.match(d, i)
    if not m:
        raise ValueError(f"bad descriptor at {i} in {d!r}")
    return m.group(0), m.end()


def parse_descriptors(d):
    """Split a descriptor string into a list of (kind, name)."""
    out, i = [], 0
    while i < len(d):
        c = d[i]
        if c == "[":
            name, i = _name(d, i + 1)
            out.append(("typeparam", name))
            i += 1  # ']'
        elif c == "(":
            name, i = _name(d, i + 1)
            out.append(("param", name))
            i += 1  # ')'
        else:
            name, i = _name(d, i)
            s = d[i]
            if s == "(":
                j = d.index(").", i)
                out.append(("method", name))
                i = j + 2
            else:
                out.append(({"/": "namespace", "#": "type", ".": "term",
                             ":": "meta", "!": "macro"}[s], name))
                i += 1
    return out


def base_type(name):
    """`Foo<T>` / `crate::a::Foo` / `&Foo` -> `Foo`."""
    name = re.split(r"[<\s(]", name.strip().lstrip("&*"))[0]
    return name.rsplit("::", 1)[-1]


class Item:
    __slots__ = ("key", "owner", "trait", "exact")

    def __init__(self, key, owner=None, trait=None, exact=False):
        self.key = key        # graph node key
        self.owner = owner    # type key for methods
        self.trait = trait    # implemented trait name, for trait-impl methods
        self.exact = exact    # the symbol *is* the item (not a member folded in)


def item_for(descs, project_types, type_keys=None):
    """Map parsed descriptors to an Item, or None for namespaces/unknown shapes.

    `project_types` is the set of type names defined in the project; it decides
    whether a trait impl method stays a separate item (project trait) or is
    folded into the implementing type (external trait such as Display).
    `type_keys` maps a type name to the keys where the symbol's package defines
    it; an impl's self type resolves to its definition when that is unique (an
    impl may live in another module), else to the impl's own module.
    """
    mods = []
    i = 0
    while i < len(descs) and descs[i][0] == "namespace":
        mods.append(descs[i][1])
        i += 1
    rest = descs[i:]
    if not rest:
        return None
    pre = "/".join(mods) + "/" if mods else ""
    kind, name = rest[0]
    # rust-analyzer: impl#[SelfType][Trait]member().
    if kind == "type" and name == "impl" and len(rest) > 1 and rest[1][0] == "typeparam":
        self_ty = base_type(rest[1][1])
        ty = pre + self_ty
        defs = (type_keys or {}).get(self_ty, ())
        if ty not in defs and len(defs) == 1:
            ty = next(iter(defs))
        j, trait = 2, None
        if j < len(rest) and rest[j][0] == "typeparam":
            trait = base_type(rest[j][1])
            j += 1
        member = next((n for k, n in rest[j:j + 1] if k in ("method", "term", "macro")), None)
        if member is None:
            return Item(ty)
        if trait and trait not in project_types:
            return Item(ty)
        return Item(f"{pre}{self_ty}::{member}", ty, trait, exact=len(rest) == j + 1)
    if kind in ("typeparam", "param", "meta"):
        return None
    if kind == "type" and len(rest) > 1 and rest[1][0] == "method":
        return Item(f"{pre}{name}::{rest[1][1]}", pre + name, None, exact=len(rest) == 2)
    return Item(pre + name, exact=len(rest) == 1)

