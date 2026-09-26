"""Sqlon 3 reference-compatible Python implementation."""
from __future__ import annotations

import base64
import math
import re
import struct
from dataclasses import dataclass
from typing import Any

MAX_BYTES = 64 * 1024 * 1024
MAX_ITEMS = 1_000_000
MAX_DEPTH = 128
_NUMBER = re.compile(r"-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?\Z", re.ASCII)
_NAME = re.compile(r"[a-z][a-z0-9_]*\Z", re.ASCII)
_EXT_VALUE = re.compile(r"[A-Za-z0-9._-]+\Z", re.ASCII)


@dataclass(frozen=True)
class Schema:
    kind: str
    size: int | None = None
    children: tuple[Schema, ...] = ()
    key_bytes: int | None = None


@dataclass
class Value:
    kind: str
    value: Any = None


class _SchemaParser:
    def __init__(self, source: str):
        self.source = source
        self.pos = 0

    def count(self, required: bool) -> int | None:
        start = self.pos
        while self.pos < len(self.source) and self.source[self.pos].isdigit():
            self.pos += 1
        if start == self.pos:
            if required:
                raise ValueError("missing schema count")
            return None
        token = self.source[start:self.pos]
        if len(token) > 1 and token[0] == "0":
            raise ValueError("leading zero in schema count")
        count = int(token)
        if count > MAX_BYTES:
            raise ValueError("schema count exceeds limit")
        return count

    def value(self, depth: int = 0) -> Schema:
        if depth > MAX_DEPTH or self.pos >= len(self.source):
            raise ValueError("invalid schema tree")
        kind = self.source[self.pos]
        self.pos += 1
        if kind in "NBIDM":
            return Schema(kind)
        if kind in "SX":
            return Schema(kind, self.count(False))
        if kind == "A":
            return Schema(kind, children=(self.value(depth + 1),))
        if kind in "LO":
            count = self.count(True)
            if count > MAX_ITEMS:
                raise ValueError("container too large")
            key_bytes = None
            if kind == "O" and self.source[self.pos:self.pos + 1] == "K":
                self.pos += 1
                key_bytes = self.count(True)
            children = tuple(self.value(depth + 1) for _ in range(count))
            return Schema(kind, children=children, key_bytes=key_bytes)
        raise ValueError("unknown schema code")


def _schema_text(s: Schema) -> str:
    if s.kind in "NBIDM":
        return s.kind
    if s.kind in "SX":
        return s.kind + ("" if s.size is None else str(s.size))
    if s.kind == "A":
        return "A" + _schema_text(s.children[0])
    if s.kind in "LO":
        key = "" if s.key_bytes is None else "K" + str(s.key_bytes)
        return s.kind + str(len(s.children)) + key + "".join(map(_schema_text, s.children))
    raise ValueError("bad schema")


@dataclass(frozen=True)
class Document:
    width: int
    root: Schema

    @classmethod
    def parse(cls, source: str) -> Document:
        if not source.isascii() or ":" not in source:
            raise ValueError("schema must be ASCII with header colon")
        head, body = source.split(":", 1)
        if not head.startswith("@3") or len(head) < 3:
            raise ValueError("unsupported version")
        if head[2] not in "248":
            raise ValueError("bad width")
        tail = head[3:]
        if tail:
            if not tail.startswith(";"):
                raise ValueError("bad header")
            names = set()
            for part in tail[1:].split(";"):
                if "=" not in part:
                    raise ValueError("bad extension")
                name, value = part.split("=", 1)
                if not _NAME.fullmatch(name) or not _EXT_VALUE.fullmatch(value) or name in names:
                    raise ValueError("bad extension")
                names.add(name)
                raise ValueError("unknown extension")
        parser = _SchemaParser(body)
        root = parser.value()
        if parser.pos != len(body):
            raise ValueError("trailing schema bytes")
        return cls(int(head[2]), root)

    def text(self) -> str:
        if self.width not in (2, 4, 8):
            raise ValueError("bad width")
        return "@3" + str(self.width) + ":" + _schema_text(self.root)

    def encode(self, value: Value) -> bytes:
        out = bytearray()
        _encode(self.root, value, self.width, out, 0)
        if len(out) > MAX_BYTES:
            raise ValueError("data too large")
        return bytes(out)

    def decode(self, data: bytes) -> Value:
        if len(data) > MAX_BYTES:
            raise ValueError("data too large")
        raw = data
        reader = _Reader(raw, self.width)
        value = _decode(self.root, reader, 0)
        if reader.pos != len(raw):
            raise ValueError("trailing data")
        return value


def _put_len(out: bytearray, length: int, width: int) -> None:
    if length < 0 or length >= 1 << (8 * width):
        raise ValueError("length exceeds width")
    out.extend(length.to_bytes(width, "little"))


def _encode(s: Schema, v: Value, width: int, out: bytearray, depth: int) -> None:
    if depth > MAX_DEPTH or s.kind != v.kind:
        raise ValueError("value does not match schema")
    k, data = s.kind, v.value
    if k == "N":
        pass
    elif k == "B":
        out.append(ord("T" if data else "F"))
    elif k == "I":
        out.extend(int(data).to_bytes(8, "little", signed=True))
    elif k == "D":
        if not math.isfinite(data):
            raise ValueError("non-finite double")
        out.extend(struct.pack("<d", data))
    elif k == "M":
        if not isinstance(data, str) or not _NUMBER.fullmatch(data):
            raise ValueError("invalid decimal")
        raw = data.encode("ascii")
        _put_len(out, len(raw), width)
        out.extend(raw)
    elif k in "SX":
        raw = data.encode("utf-8") if k == "S" else bytes(data)
        if s.size is None:
            _put_len(out, len(raw), width)
        elif len(raw) != s.size:
            raise ValueError("fixed length mismatch")
        out.extend(raw)
    elif k == "L":
        if len(data) != len(s.children):
            raise ValueError("list length mismatch")
        for child, item in zip(s.children, data):
            _encode(child, item, width, out, depth + 1)
    elif k == "O":
        if len(data) != len(s.children):
            raise ValueError("object length mismatch")
        seen = set()
        for child, (key, item) in zip(s.children, data):
            if key in seen:
                raise ValueError("duplicate key")
            seen.add(key)
            raw = key.encode("utf-8")
            if s.key_bytes is None:
                _put_len(out, len(raw), width)
            elif len(raw) != s.key_bytes:
                raise ValueError("fixed key length mismatch")
            out.extend(raw)
            _encode(child, item, width, out, depth + 1)
    elif k == "A":
        if len(data) > MAX_ITEMS:
            raise ValueError("array too large")
        _put_len(out, len(data), width)
        for item in data:
            _encode(s.children[0], item, width, out, depth + 1)
    if len(out) > MAX_BYTES:
        raise ValueError("data too large")


class _Reader:
    def __init__(self, data: bytes, width: int):
        self.data = data
        self.width = width
        self.pos = 0

    def take(self, n: int) -> bytes:
        if n < 0 or n > len(self.data) - self.pos:
            raise ValueError("truncated data")
        result = self.data[self.pos:self.pos + n]
        self.pos += n
        return result

    def length(self) -> int:
        return int.from_bytes(self.take(self.width), "little")


def _decode(s: Schema, r: _Reader, depth: int) -> Value:
    if depth > MAX_DEPTH:
        raise ValueError("nesting limit")
    k = s.kind
    if k == "N":
        return Value(k)
    if k == "B":
        token = r.take(1)
        if token not in (b"T", b"F"):
            raise ValueError("invalid bool")
        return Value(k, token == b"T")
    if k == "I":
        return Value(k, int.from_bytes(r.take(8), "little", signed=True))
    if k == "D":
        value = struct.unpack("<d", r.take(8))[0]
        if not math.isfinite(value):
            raise ValueError("non-finite double")
        return Value(k, value)
    if k == "M":
        count = r.length()
        if not 0 < count <= MAX_BYTES:
            raise ValueError("invalid decimal length")
        raw = r.take(count)
        try:
            text = raw.decode("ascii")
        except UnicodeDecodeError as exc:
            raise ValueError("invalid decimal") from exc
        if not _NUMBER.fullmatch(text):
            raise ValueError("invalid decimal")
        return Value(k, text)
    if k in "SX":
        count = r.length() if s.size is None else s.size
        if count > MAX_BYTES:
            raise ValueError("value too large")
        raw = r.take(count)
        return Value(k, raw.decode("utf-8") if k == "S" else raw)
    if k == "L":
        return Value(k, [_decode(child, r, depth + 1) for child in s.children])
    if k == "O":
        entries, seen = [], set()
        for child in s.children:
            count = r.length() if s.key_bytes is None else s.key_bytes
            if count > MAX_BYTES:
                raise ValueError("key too large")
            key = r.take(count).decode("utf-8")
            if key in seen:
                raise ValueError("duplicate key")
            seen.add(key)
            entries.append((key, _decode(child, r, depth + 1)))
        return Value(k, entries)
    if k == "A":
        count = r.length()
        if count > MAX_ITEMS:
            raise ValueError("array too large")
        return Value(k, [_decode(s.children[0], r, depth + 1) for _ in range(count)])
    raise ValueError("bad schema")


def _json_string(text: str) -> str:
    parts = ['"']
    escapes = {'"': '\\"', "\\": "\\\\", "\b": "\\b", "\t": "\\t",
               "\n": "\\n", "\f": "\\f", "\r": "\\r"}
    for char in text:
        parts.append(escapes.get(char) or (f"\\u{ord(char):04x}" if ord(char) < 32 else char))
    parts.append('"')
    return "".join(parts)


def _double_exact(value: float) -> str:
    if not math.isfinite(value):
        raise ValueError("non-finite double")
    if value == 0:
        return "-0" if math.copysign(1, value) < 0 else "0"
    sign = "-" if value < 0 else ""
    numerator, denominator = abs(value).as_integer_ratio()
    places = denominator.bit_length() - 1
    digits = str(numerator * 5 ** places)
    if places:
        digits = digits.zfill(places + 1)
        digits = digits[:-places] + "." + digits[-places:]
        digits = digits.rstrip("0").rstrip(".")
    return sign + digits


def to_json(value: Value) -> str:
    def render(v: Value, depth: int) -> str:
        if depth > MAX_DEPTH:
            raise ValueError("nesting limit")
        k, x = v.kind, v.value
        if k == "N": return "null"
        if k == "B": return "true" if x else "false"
        if k == "I": return str(x)
        if k == "D": return _double_exact(x)
        if k == "M":
            if not _NUMBER.fullmatch(x): raise ValueError("invalid decimal")
            return x
        if k == "S": return _json_string(x)
        if k == "X": return _json_string(base64.b64encode(x).decode("ascii"))
        if k in "LA": return "[" + ",".join(render(y, depth + 1) for y in x) + "]"
        if k == "O":
            seen = set()
            pieces = []
            for key, item in x:
                if key in seen: raise ValueError("duplicate key")
                seen.add(key)
                pieces.append(_json_string(key) + ":" + render(item, depth + 1))
            return "{" + ",".join(pieces) + "}"
        raise ValueError("bad value")
    return render(value, 0)


def pack(document: Document, data: bytes) -> bytes:
    outer = Document.parse("@38:L2SX")
    return outer.encode(Value("L", [Value("S", document.text()), Value("X", data)]))


def unpack(data: bytes) -> tuple[Document, bytes]:
    outer = Document.parse("@38:L2SX")
    schema, raw = outer.decode(data).value
    document = Document.parse(schema.value)
    document.decode(raw.value)
    return document, raw.value
from dataclasses import dataclass

@dataclass(frozen=True)
class EncodeOptions:
    fixed_strings: bool = False
    fixed_bytes: bool = False
    homogeneous_arrays: bool = False
    fixed_keys: bool = False
    compact_lengths: bool = False
    width: int = 8

    @classmethod
    def optimized(cls, **kwargs):
        return cls(**(dict(fixed_strings=True, fixed_bytes=True, homogeneous_arrays=True, fixed_keys=True, compact_lengths=True) | kwargs))

@dataclass(frozen=True)
class Encoded:
    document: Document
    data: bytes


def encode_auto(value: Value, options: EncodeOptions = EncodeOptions()) -> Encoded:
    """Infer Schema, optionally minimize Schema + Data; then encode.

    This is a size optimizer, not a promise of faster encoding. Explicit Document
    encoding and all decoding remain unchanged. Ties retain the baseline layout.
    """
    if options.width not in (2, 4, 8):
        raise ValueError("bad width")
    def plan(width):
        cache = {}
        def infer(vs, depth=0):
            if depth > MAX_DEPTH:
                raise ValueError("nesting limit")
            key = tuple(map(id, vs))
            if key in cache:
                return cache[key]
            result = build(vs, depth)
            cache[key] = result
            return result
        def build(vs, depth):
            k = vs[0].kind
            if k in "LA": k = "L"
            if any(("L" if v.kind in "LA" else v.kind) != k for v in vs): return None
            n = len(vs)
            if k in "NBID": return Schema(k), n * ({"N":0,"B":1,"I":8,"D":8}[k]), 0
            if k in "SXM":
                lengths = [len(v.value.encode("utf-8")) if k != "X" else len(v.value) for v in vs]
                fixed = k != "M" and (options.fixed_strings if k == "S" else options.fixed_bytes)
                size = lengths[0] if fixed and len(set(lengths)) == 1 and len(str(lengths[0])) < width*n else None
                return Schema(k, size), sum(lengths)+(width*n if size is None else 0), max(lengths) if size is None else 0
            if k not in "LO": raise ValueError("bad value kind")
            rows = [v.value for v in vs]
            if any(len(row)>MAX_ITEMS for row in rows): raise ValueError("container too large")
            candidate = None
            if len({len(row) for row in rows}) == 1:
                children=[]; total=0; maximum=0
                for i in range(len(rows[0])):
                    p=infer([row[i][1] if k=="O" else row[i] for row in rows],depth+1)
                    if p is None: break
                    children.append(p[0]); total+=p[1]; maximum=max(maximum,p[2])
                else:
                    key_size=None
                    if k=="O":
                        lengths=[len(key.encode("utf-8")) for row in rows for key,_ in row]
                        if lengths:
                            if options.fixed_keys and len(set(lengths))==1 and 1+len(str(lengths[0])) < width*len(lengths): key_size=lengths[0]
                            total+=sum(lengths)+(width*len(lengths) if key_size is None else 0)
                            if key_size is None: maximum=max(maximum,max(lengths))
                    candidate=(Schema(k,children=tuple(children),key_bytes=key_size),total,maximum)
            if k=="L" and options.homogeneous_arrays:
                flat=[v for row in rows for v in row]
                if flat:
                    p=infer(flat,depth+1)
                    if p is not None:
                        alternative=(Schema("A",children=(p[0],)),p[1]+width*n,max(p[2],max(map(len,rows))))
                        if candidate is None or len(_schema_text(alternative[0]))+alternative[1] < len(_schema_text(candidate[0]))+candidate[1]: candidate=alternative
            return candidate
        return infer([value])
    chosen=None
    for width in ((2,4,8) if options.compact_lengths else (options.width,)):
        p=plan(width)
        if p is None: raise ValueError("cannot infer schema")
        if p[2] < 1 << (width*8):
            chosen=Document(width,p[0]); break
    if chosen is None: raise ValueError("length exceeds width")
    def adapt(s,v):
        if s.kind in "LA":
            return Value(s.kind,[adapt(s.children[0] if s.kind=="A" else s.children[i],x) for i,x in enumerate(v.value)])
        if s.kind=="O": return Value("O",[(key,adapt(s.children[i],x)) for i,(key,x) in enumerate(v.value)])
        return v
    return Encoded(chosen,chosen.encode(adapt(chosen.root,value)))
