"""Minimal SCIP protobuf decoder (no protobuf dependency)."""


def varint(buf, i):
    shift = result = 0
    while True:
        b = buf[i]
        i += 1
        result |= (b & 0x7F) << shift
        if not b & 0x80:
            return result, i
        shift += 7


def fields(buf):
    i, n = 0, len(buf)
    while i < n:
        key, i = varint(buf, i)
        fno, wt = key >> 3, key & 7
        if wt == 0:
            v, i = varint(buf, i)
        elif wt == 2:
            ln, i = varint(buf, i)
            v = buf[i:i + ln]
            i += ln
        elif wt == 1:
            v = buf[i:i + 8]; i += 8
        elif wt == 5:
            v = buf[i:i + 4]; i += 4
        else:
            raise ValueError(wt)
        yield fno, wt, v


def packed(buf):
    out, i = [], 0
    while i < len(buf):
        v, i = varint(buf, i)
        out.append(v)
    return out


def occurrence(buf):
    o = {"range": [], "symbol": "", "roles": 0, "enclosing": []}
    for f, wt, v in fields(buf):
        if f == 1:
            o["range"] += packed(v) if wt == 2 else [v]
        elif f == 2:
            o["symbol"] = v.decode()
        elif f == 3:
            o["roles"] = v
        elif f == 7:
            o["enclosing"] += packed(v) if wt == 2 else [v]
    return o


def symbol_info(buf):
    s = {"symbol": "", "kind": 0, "display": "", "enclosing_symbol": "", "rels": []}
    for f, wt, v in fields(buf):
        if f == 1:
            s["symbol"] = v.decode()
        elif f == 5:
            s["kind"] = v
        elif f == 6:
            s["display"] = v.decode()
        elif f == 8:
            s["enclosing_symbol"] = v.decode()
    return s


def document(buf):
    d = {"path": "", "occ": [], "syms": []}
    for f, wt, v in fields(buf):
        if f == 1:
            d["path"] = v.decode()
        elif f == 2:
            d["occ"].append(occurrence(v))
        elif f == 3:
            d["syms"].append(symbol_info(v))
    return d


def load(path):
    data = open(path, "rb").read()
    return [document(v) for f, wt, v in fields(data) if f == 2]
