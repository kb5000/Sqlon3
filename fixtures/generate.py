from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
from sqlon3 import Document, Value, to_json

root = Path(__file__).resolve().parent
doc = Document.parse("@34:O4K3IASXM")
value = Value("O", [
    ("num", Value("I", -42)),
    ("arr", Value("A", [Value("S", "a"), Value("S", "中")])),
    ("bin", Value("X", bytes([0, 255, 66]))),
    ("dec", Value("M", "12345678901234567890.0001")),
])
(root / "full.schema").write_text(doc.text(), encoding="ascii")
(root / "full.bin").write_bytes(doc.encode(value))
(root / "full.json").write_text(to_json(value), encoding="utf-8")

from sqlon3 import EncodeOptions, encode_auto
rows = Value("L", [Value("O", [("num", Value("I", i)), ("txt", Value("S", "中"))]) for i in range(10)])
optimized = encode_auto(rows, EncodeOptions.optimized())
(root / "optimized.schema").write_text(optimized.document.text(), encoding="ascii")
(root / "optimized.bin").write_bytes(optimized.data)
(root / "optimized.json").write_text(to_json(rows), encoding="utf-8")
