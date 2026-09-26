# Sqlon 3: A Schema/Data Binary Serialization Format

- Status: Project specification
- Version: 3
- Date: 2026-09-26

Chinese translation: [SPEC.zh-CN.md](SPEC.zh-CN.md). This English document is normative; if the two differ, this document prevails.

## Abstract

Sqlon 3 represents structured values using a human-readable Schema and binary Data. A Schema can be reused with multiple Data sequences. The protocol covers the six JSON value kinds and adds a raw Bytes type. This document defines the syntax, wire layout, validation rules, JSON conversion, and a packing convention. It is the single source of truth for Sqlon 3. Compatibility with Sqlon 2 is not required.

## 1. Conventions and Terminology

The key words **MUST**, **MUST NOT**, **SHOULD**, and **MAY** in this document are to be interpreted as described in [BCP 14](https://www.rfc-editor.org/info/rfc8174/) when they appear in all capitals. A byte is an octet. Unless otherwise stated, a length counts bytes. A “Schema” is the complete ASCII byte sequence consisting of a header and one value schema. “Data” is the complete byte sequence paired with it.

A document consists of separate Schema and Data sequences. A transport or container provides their boundaries; Sqlon 3 does not define a file envelope. Section 7 defines an optional packing convention that transmits only one outer Data sequence.

## 2. Data Model

Sqlon 3 has ten encoding types: Null, Bool, Int, Double, Decimal, String, Bytes, List, Object, and Array. Null, Bool, String, List, and Object map to JSON null, boolean, string, array, and object, respectively. Array also maps to a JSON array. Int, Double, and Decimal all map to JSON numbers. Bytes is an additional raw-binary type; Section 6 defines its JSON conversion.

The encoding types and JSON types are not in one-to-one correspondence. Different valid Sqlon 3 encodings **MAY** produce the same JSON value. A given valid Schema/Data pair **MUST** produce deterministic JSON text, except for the order of Object members.

## 3. Schema Header

### 3.1. Header

The entire Schema **MUST** contain only ASCII bytes and **MUST** begin with:

```text
@3<w>[;name=value]*:<value-schema>
```

Here `w` is exactly `2`, `4`, or `8` and selects the width, in bytes, of Data length fields. With no extensions, the header `@3<w>:` is exactly four bytes. For example, `@32:AN` has value schema `AN`.

The version marker is `@3`. A decoder **MUST** reject any other version or width and **MUST NOT** guess a substitute meaning.

Extensions are optional `;name=value` entries between the width and the colon. A `name` **MUST** match `[a-z][a-z0-9_]*`; a `value` **MUST** match `[A-Za-z0-9._-]+`. Names **MUST NOT** repeat. A decoder **MUST** reject an unknown extension: it might affect decoding and cannot be silently ignored. This version defines no extensions. Extension semantics must be defined by a later revision of this specification or agreed upon by both communicating parties.
### 3.2. Length Fields

Every length or element-count field in Data **MUST** be an unsigned little-endian integer of the width selected in the header. The maximum values for widths 2, 4, and 8 are 65535, 4294967295, and 18446744073709551615, respectively. An encoder **MUST NOT** truncate an out-of-range value. Int and Double are always eight bytes; the width option does not affect them. Decimal counts written in the Schema are independent of this width, although implementations may apply resource limits.

### 3.3. Transport Processing

Compression is outside the Sqlon 3 encoding. A transport or container may compress a message, but it must restore the original Schema and Data bytes before Sqlon decoding. This specification defines no compression extension or algorithm.

## 4. Value Schema Syntax

The notation below is descriptive: angle brackets, square brackets, and ellipses are not wire bytes. Every `u` is a nonnegative ASCII decimal integer without leading zeroes; zero is written only as `0`.

| Type | Value schema | Following child schemas |
| --- | --- | --- |
| Null | `N` | None |
| Bool | `B` | None |
| Int | `I` | None |
| Double | `D` | None |
| Decimal | `M` | None |
| String | `S` or `S<u>` | None |
| Bytes | `X` or `X<u>` | None |
| List | `L<u>` | Exactly `u`, in order |
| Object | `O<u>` or `O<u>K<u>` | Exactly as many as the first `u`, in order |
| Array | `A` | Exactly one element schema |

`S` and `X` without a following number are variable length. `S0` and `X0` are fixed-length empty values. `K<u>` is a fixed-key-length modifier that can appear only immediately after an Object's member count; it is not a value type. The two `u` values in `O<u>K<u>` are the member count and each key's UTF-8 byte length, respectively.

A value schema **MUST** contain exactly one complete tree. A decoder **MUST** reject unknown type codes, missing children, malformed numbers, trailing Schema bytes, and schemas that exceed its depth or size limits.

## 5. Data Encoding

| Type | Encoding |
| --- | --- |
| Null | Zero bytes |
| Bool | One ASCII byte, `T` or `F` |
| Int | Signed 64-bit two's-complement integer, little-endian, eight bytes |
| Double | IEEE 754 binary64 bit pattern, little-endian, eight bytes |
| Decimal | Length field followed by that many ASCII JSON-number bytes |
| String | Variable: length field followed by UTF-8 bytes; fixed: UTF-8 bytes only |
| Bytes | Variable: length field followed by raw bytes; fixed: raw bytes only |
| List | Child Data concatenated in schema order; no own length field |
| Object | Keys and associated value Data concatenated in schema order |
| Array | Element-count field followed by each element encoded using the same element schema |

A Decimal text **MUST** contain at least one byte and **MUST** match the JSON number grammar of [RFC 8259](https://www.rfc-editor.org/info/rfc8259/) in its entirety: `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`. A Double with a NaN or positive or negative infinity bit pattern **MUST** be rejected. Strings and Object keys **MUST** be valid UTF-8 representing Unicode scalar values. Bytes have no UTF-8 restriction.

For an Object without `K<u>`, each member is encoded as a key-length field, key UTF-8 bytes, and value Data. With `K<u>`, each key **MUST** occupy exactly `u` UTF-8 bytes; no key-length field, padding, or truncation is allowed. `K0` permits an empty key. This modifier applies only to its enclosing Object, not nested Objects. Its `u` is a Schema number and is not limited by the selected Data length width. Decoded keys within one Object **MUST** be unique. Keys and value schemas correspond by position; an encoder **MUST** generate Schema and Data in matching order.

A List has a fixed count and a schema for each position, so its elements may be heterogeneous. An Array stores its count in Data; the count may change between Data sequences, but every element **MUST** conform to its sole element schema. Even an empty Array **MUST** have an element schema, for example `AN`. When that schema is `N`, each element occupies zero bytes; the count still determines the JSON array length.

A decoder **MUST** consume the Data exactly. It **MUST** reject truncation, length overflow, invalid markers, duplicate keys, invalid UTF-8, invalid Decimals, values that do not match the Schema, and trailing bytes.

## 6. JSON Conversion

Conversion from Sqlon 3 to JSON **MUST** produce UTF-8 JSON text conforming to [RFC 8259](https://www.rfc-editor.org/info/rfc8259/):

1. Null, Bool, String, List, Object, and Array produce JSON null, boolean, string, array, object, and array, respectively.
2. Int produces a decimal integer without leading zeroes.
3. Decimal produces its stored JSON-number text verbatim, without a binary64 round trip.
4. Double produces the **exact** decimal expansion of its finite binary64 value, without exponent notation. Trailing zeroes in the fractional part are removed; an integer has no decimal point; negative zero is `-0`. The result can be long, but is identical across implementations.
5. Bytes produces a Base64 JSON string using the standard alphabet of [RFC 4648, Section 4](https://www.rfc-editor.org/info/rfc4648/), with required `=` padding, no line breaks, and zero unused pad bits. This applies only to JSON conversion: Sqlon Data stores the original bytes.
6. In JSON strings and Object keys, a quotation mark is rendered as `\"` and a reverse solidus as `\\`. U+0008, U+0009, U+000A, U+000C, and U+000D are rendered as `\b`, `\t`, `\n`, `\f`, and `\r`, respectively. Other U+0000 through U+001F characters use lowercase hexadecimal `\u00xx`. All other Unicode scalar values are emitted directly as UTF-8. A generator **MUST NOT** add a BOM or insignificant whitespace.

Object member order **MAY** differ. String and Bytes remain different Sqlon 3 types even when they produce the same JSON string. This specification does not require a unique JSON-to-Sqlon encoding. The grammar in RFC 8259 can admit isolated UTF-16 surrogate escapes, whose interoperability is unpredictable; Sqlon 3 Strings and Object keys accept only Unicode scalar values. This UTF-8 requirement is a Sqlon 3 wire rule, not a claim that a JSON string must be stored as UTF-8 in memory.

## 7. Packing Schema and Data Together

To send an inner Schema and its associated inner Data in one binary message, a sender **MAY** use the fixed outer Schema `@38:L2SX` and transmit only its outer Data. The first List member, `S`, holds the complete ASCII inner Schema. The second, `X`, holds the inner Data as raw bytes. Both members are variable length and therefore each has an eight-byte little-endian length field. Both outer and inner Data follow the ordinary encoding rules in Section 5.

A recipient first decodes the two members using the fixed outer Schema, then validates and decodes the inner Data using the inner Schema. It **MUST** check boundaries and validity at both layers. This convention adds no value type and is not required for every Sqlon 3 document.

## 8. Interoperability Examples

Spaces between hexadecimal bytes in this section are for display only; they are not part of Data.

| Schema | Data (hex) | Decoded value / JSON |
| --- | --- | --- |
| `@32:N` | Empty | `null` |
| `@32:AN` | `03 00` | `[null,null,null]` |
| `@32:L2NN` | Empty | `[null,null]` |
| `@32:X` | `03 00 00 FF 42` | Bytes `00 FF 42` / `"AP9C"` |
| `@32:X3` | `00 FF 42` | Same value |
| `@32:O1K3I` | `61 62 63 01 00 00 00 00 00 00 00` | `{"abc":1}` |

The selected width changes only length and count fields in Data; it does not change the eight-byte encoding of Int or Double.

## 9. Security and Resource Limits

Implementations **SHOULD** limit Schema depth, container counts, byte lengths, total Data size, and check length arithmetic before allocating memory. A decoder **MUST NOT** treat unknown extensions as harmless hints. Sqlon 3 provides no encryption, authentication, or integrity protection; applications needing those properties must provide them at another layer.

## 10. References

- [BCP 14 / RFC 8174: Requirements-language key words](https://www.rfc-editor.org/info/rfc8174/)
- [RFC 8259: The JSON Data Interchange Format](https://www.rfc-editor.org/info/rfc8259/)
- [RFC 4648: Base64 Encoding](https://www.rfc-editor.org/info/rfc4648/)

## Appendix A. Optional Encoder Policies (Informative)

An encoder may offer explicit options for choosing a Schema from values. These
policies do not add wire types, header flags, or decoder modes. The transmitted
Schema completely specifies decoding. A size-optimized encoding must preserve
JSON values (and Bytes values); List and Array may represent the same JSON array.
Applications requiring a stable, reusable Schema should supply that Schema explicitly.

The implementations in this repository provide independent, disabled-by-default
options for fixed String lengths, fixed Bytes lengths, homogeneous Arrays, fixed
object-key lengths, and compact length widths. An `optimized()` options package
enables all five. Compression belongs to the outer transport or container.

- Fixed String and key lengths are measured in UTF-8 bytes, not characters. Fixed
  Bytes lengths are measured in raw bytes. No truncation or padding is permitted.
- Homogeneous Array selection requires a common Schema for every element,
  recursively. Equal primitive type labels alone are not sufficient for nested
  containers. Variable string lengths can share `S`; matching lengths can share
  `S<n>`. Incompatible elements retain a heterogeneous List layout.
- Local layout choices compare Schema bytes plus aggregate Data
  bytes. Fixed fields must save more length-prefix bytes than their additional
  Schema digits cost. Array choices include their count prefixes. Equal-size
  choices retain the baseline layout. This policy does not guarantee a globally
  minimal encoding.
- Compact widths try 2, 4, then 8 bytes. A width is usable only when every length
  or count actually written in Data fits its unsigned range. A fixed-length
  field has no length prefix; Int and Double remain eight bytes regardless.

Automatic encoding includes an analysis stage before Data encoding, and may
allocate intermediate structures. It optimizes representation size, not CPU
latency. Schema-driven Data encoding and decoding remain sequential operations.
