# Sqlon 3

Sqlon 3 serializes structured values as a readable Schema plus binary Data. It covers JSON values and adds raw Bytes.

- **Normative specification:** [SPEC.md](SPEC.md)
- **中文译本：** [SPEC.zh-CN.md](SPEC.zh-CN.md)

The English specification is the single source of truth. This repository contains a Rust reference implementation and independent C++, Java, Python, and Node.js implementations. The Sqlon 2 source has been removed.

| Language | Implementation |
| --- | --- |
| Rust | [rust/src/lib.rs](rust/src/lib.rs) |
| C++17 | [cpp/Sqlon3.h](cpp/Sqlon3.h), [cpp/Sqlon3.cpp](cpp/Sqlon3.cpp) |
| Java 17 | [java/src/Sqlon3.java](java/src/Sqlon3.java) |
| Python 3.10+ | [python/sqlon3.py](python/sqlon3.py) |
| Node.js | [node/sqlon3.js](node/sqlon3.js) |

Each implementation provides Schema parsing, Data encoding and decoding, deterministic JSON rendering, and the fixed `@38:L2SX` packing convention. The current implementations cap individual Data sequences at 64 MiB and container counts at one million elements.

## Build and test

From the repository root:

```sh
cargo test --manifest-path rust/Cargo.toml
python -m unittest discover -s python -v
npm test --prefix node
cmake -S cpp -B cpp/build
cmake --build cpp/build --config Debug
ctest --test-dir cpp/build -C Debug --output-on-failure
javac -encoding UTF-8 -d java/build java/src/Sqlon3.java java/src/Sqlon3Optimizer.java java/src/TestSqlon3.java java/src/TestOptimization.java
java -cp java/build TestSqlon3
java -cp java/build TestOptimization
```

C++, Java, Python, and Node.js use only their standard libraries. Rust uses `base64` and `num-bigint` for JSON output. An optional Maven project is provided in `java/pom.xml`.

The shared [fixture](fixtures/full.schema) and its [binary Data](fixtures/full.bin) cover nested objects, arrays, UTF-8 strings, raw bytes, and Decimal. All five implementations decode it to the same [JSON result](fixtures/full.json). The fixture can be regenerated with [fixtures/generate.py](fixtures/generate.py).

## Minimal example

A Schema `@32:AN` with Data bytes `03 00` represents `[null,null,null]`. Here `2` selects two-byte little-endian lengths, `A` is a variable-count array, and `N` is a zero-byte Null element.

## Optional size optimization

Schema-driven `Document.encode` remains explicit and unchanged. Use the automatic
encoder when the Schema may be inferred from a value. Its default options disable
optimizations and use width 8; JSON arrays initially use heterogeneous Lists.
`optimized()` enables fixed strings, fixed bytes, homogeneous arrays, fixed keys,
and compact lengths. Each flag can also be enabled independently.

| Language | Automatic encoder | Full options package |
| --- | --- | --- |
| Rust | `encode_auto(&value, &options)` | `EncodeOptions::optimized()` |
| C++ | `encode_auto(value, options)` | `EncodeOptions::optimized()` |
| Java | `Sqlon3Optimizer.encodeAuto(value, options)` | `Sqlon3Optimizer.EncodeOptions.optimized()` |
| Python | `encode_auto(value, options)` | `EncodeOptions.optimized()` |
| Node.js | `encodeAuto(value, options)` | `EncodeOptions.optimized()` |

The result contains `document` (the selected Schema) and `data`. Send or retain
both. Decode with `document.decode(data)`; no optimization option is needed.
The automatic encoder accepts both List and Array input values as JSON arrays.

Python example:

```python
from sqlon3 import Value, EncodeOptions, encode_auto

v = Value("L", [Value("I", i) for i in range(100)])
encoded = encode_auto(v, EncodeOptions.optimized())
print(encoded.document.text())  # @32:AI
restored = encoded.document.decode(encoded.data)

# Enable only compact length prefixes.
encoded = encode_auto(v, EncodeOptions(compact_lengths=True))
# Override an individual flag in the complete package.
options = EncodeOptions.optimized(fixed_strings=False)
```

Rust imports these APIs directly from `sqlon3`; C++ declares them in `Sqlon3.h`.
Java options are an immutable record, with fields in this order: `fixedStrings`,
`fixedBytes`, `homogeneousArrays`, `fixedKeys`, `compactLengths`, `width`. Rust and C++ expose mutable fields in snake_case. Node.js accepts
camelCase fields and supports `EncodeOptions.optimized({ fixedStrings: false })`.

Layout comparisons use Schema plus Data size; equal-size alternatives
retain the baseline. Compact lengths select the smallest usable width in 2/4/8.
Compression is handled by an outer transport or container; no compression API or header extension is defined by Sqlon 3. Optimization adds
analysis work and intermediate allocations, so it is not a CPU-speed preset and
does not promise global minimum size. Explicit Schema reuse avoids this analysis.
See [SPEC Appendix A](SPEC.md#appendix-a-optional-encoder-policies-informative).

All five implementations also encode the same [optimized fixture](fixtures/optimized.schema)
to identical [Data bytes](fixtures/optimized.bin). Java's additional optimization
test entry point is `TestOptimization`.
