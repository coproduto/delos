"""Minimal SCIP index decoder (no protobuf dependency).

Only the fields refgraph needs are decoded. Schema reference:
https://github.com/sourcegraph/scip/blob/main/scip.proto
"""

DEFINITION_ROLE = 1


def _varint(buf, i):
    shift = result = 0
    while True:
        b = buf[i]
        i += 1
        result |= (b & 0x7F) << shift
        if not b & 0x80:
            return result, i
        shift += 7


def _fields(buf):
    i, n = 0, len(buf)
    while i < n:
        key, i = _varint(buf, i)
        fno, wt = key >> 3, key & 7
        if wt == 0:
            v, i = _varint(buf, i)
        elif wt == 2:
            ln, i = _varint(buf, i)
            v = buf[i:i + ln]
            i += ln
        elif wt == 1:
            v = buf[i:i + 8]
            i += 8
        elif wt == 5:
            v = buf[i:i + 4]
            i += 4
        else:
            raise ValueError(f"unsupported protobuf wire type {wt}")
        yield fno, wt, v


def _packed(buf):
    out, i = [], 0
    while i < len(buf):
        v, i = _varint(buf, i)
        out.append(v)
    return out


def _range(r):
    """SCIP ranges are [line, start, end] or [sl, sc, el, ec]; normalise to 4."""
    if len(r) == 3:
        return (r[0], r[1], r[0], r[2])
    if len(r) == 4:
        return tuple(r)
    return None


def _occurrence(buf):
    o = {"range": [], "symbol": "", "roles": 0, "enclosing": []}
    for f, wt, v in _fields(buf):
        if f == 1:
            o["range"] += _packed(v) if wt == 2 else [v]
        elif f == 2:
            o["symbol"] = v.decode()
        elif f == 3:
            o["roles"] = v
        elif f == 7:
            o["enclosing"] += _packed(v) if wt == 2 else [v]
    o["range"] = _range(o["range"])
    o["enclosing"] = _range(o["enclosing"]) if o["enclosing"] else None
    return o


def _symbol_info(buf):
    s = {"symbol": "", "kind": 0, "display": ""}
    for f, wt, v in _fields(buf):
        if f == 1:
            s["symbol"] = v.decode()
        elif f == 5:
            s["kind"] = v
        elif f == 6:
            s["display"] = v.decode()
    return s


def _document(buf):
    d = {"path": "", "occurrences": [], "symbols": []}
    for f, wt, v in _fields(buf):
        if f == 1:
            d["path"] = v.decode()
        elif f == 2:
            d["occurrences"].append(_occurrence(v))
        elif f == 3:
            d["symbols"].append(_symbol_info(v))
    return d


def load(path):
    """Return the index's documents as dicts (path, occurrences, symbols)."""
    with open(path, "rb") as f:
        data = f.read()
    return [_document(v) for f, wt, v in _fields(data) if f == 2]
