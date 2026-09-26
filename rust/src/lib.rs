use base64::{engine::general_purpose::STANDARD, Engine};
use num_bigint::BigUint;
use std::collections::HashSet;

pub type Result<T> = std::result::Result<T, String>;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ITEMS: usize = 1_000_000;
const MAX_DEPTH: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Schema {
    Null,
    Bool,
    Int,
    Double,
    Decimal,
    String(Option<usize>),
    Bytes(Option<usize>),
    List(Vec<Schema>),
    Object {
        key_bytes: Option<usize>,
        values: Vec<Schema>,
    },
    Array(Box<Schema>),
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Double(f64),
    Decimal(String),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub width: u8,
    pub root: Schema,
}

fn err<T>(message: &str) -> Result<T> {
    Err(message.into())
}
fn number_text(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.get(i) == Some(&b'-') {
        i += 1;
    }
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            while matches!(b.get(i), Some(b'0'..=b'9')) {
                i += 1;
            }
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while matches!(b.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while matches!(b.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == b.len()
}
struct SchemaParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl SchemaParser<'_> {
    fn count(&mut self, required: bool) -> Result<Option<usize>> {
        let start = self.pos;
        while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if start == self.pos {
            return if required {
                err("missing schema count")
            } else {
                Ok(None)
            };
        }
        if self.pos - start > 1 && self.bytes[start] == b'0' {
            return err("leading zero in schema count");
        }
        let mut n = 0usize;
        for &c in &self.bytes[start..self.pos] {
            n = n
                .checked_mul(10)
                .and_then(|x| x.checked_add((c - b'0') as usize))
                .ok_or("schema count overflow")?;
        }
        if n > MAX_BYTES {
            return err("schema count exceeds limit");
        }
        Ok(Some(n))
    }
    fn value(&mut self, depth: usize) -> Result<Schema> {
        if depth > MAX_DEPTH {
            return err("schema nesting limit");
        }
        let c = *self.bytes.get(self.pos).ok_or("missing schema node")?;
        self.pos += 1;
        Ok(match c {
            b'N' => Schema::Null,
            b'B' => Schema::Bool,
            b'I' => Schema::Int,
            b'D' => Schema::Double,
            b'M' => Schema::Decimal,
            b'S' => Schema::String(self.count(false)?),
            b'X' => Schema::Bytes(self.count(false)?),
            b'L' => {
                let n = self.count(true)?.unwrap();
                if n > MAX_ITEMS {
                    return err("list too large");
                }
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(self.value(depth + 1)?);
                }
                Schema::List(v)
            }
            b'O' => {
                let n = self.count(true)?.unwrap();
                if n > MAX_ITEMS {
                    return err("object too large");
                }
                let key_bytes = if self.bytes.get(self.pos) == Some(&b'K') {
                    self.pos += 1;
                    self.count(true)?
                } else {
                    None
                };
                let mut values = Vec::new();
                for _ in 0..n {
                    values.push(self.value(depth + 1)?);
                }
                Schema::Object { key_bytes, values }
            }
            b'A' => Schema::Array(Box::new(self.value(depth + 1)?)),
            _ => return err("unknown schema code"),
        })
    }
}
impl Document {
    pub fn parse(text: &str) -> Result<Self> {
        if !text.is_ascii() {
            return err("schema must be ASCII");
        }
        let (head, body) = text.split_once(':').ok_or("missing header colon")?;
        if !head.starts_with("@3") || head.len() < 3 {
            return err("unsupported version");
        }
        let width = match head.as_bytes()[2] {
            b'2' => 2,
            b'4' => 4,
            b'8' => 8,
            _ => return err("bad width"),
        };
        let mut names = HashSet::new();
        let tail = &head[3..];
        if !tail.is_empty() {
            for part in tail.split(';').skip(1) {
                let (name, value) = part.split_once('=').ok_or("bad extension")?;
                if name.is_empty()
                    || !name.as_bytes()[0].is_ascii_lowercase()
                    || !name
                        .bytes()
                        .all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == b'_')
                    || value.is_empty()
                    || !value
                        .bytes()
                        .all(|x| x.is_ascii_alphanumeric() || b"._-".contains(&x))
                    || !names.insert(name)
                {
                    return err("bad extension");
                }
                return err("unknown extension");
            }
            if !tail.starts_with(';') {
                return err("bad header");
            }
        }
        let mut p = SchemaParser {
            bytes: body.as_bytes(),
            pos: 0,
        };
        let root = p.value(0)?;
        if p.pos != body.len() {
            return err("trailing schema bytes");
        }
        Ok(Self { width, root })
    }
    pub fn text(&self) -> Result<String> {
        if !matches!(self.width, 2 | 4 | 8) {
            return err("bad width");
        }
        let mut s = format!("@3{}", self.width);
        s.push(':');
        self.root.write_schema(&mut s);
        Ok(s)
    }
    pub fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        encode_value(&self.root, value, self.width, &mut out, 0)?;
        if out.len() > MAX_BYTES {
            return err("data too large");
        }
        Ok(out)
    }
    pub fn decode(&self, data: &[u8]) -> Result<Value> {
        if data.len() > MAX_BYTES {
            return err("data too large");
        }
        let raw = data;
        let mut r = Reader {
            data: raw,
            pos: 0,
            width: self.width,
        };
        let v = decode_value(&self.root, &mut r, 0)?;
        if r.pos != raw.len() {
            return err("trailing data bytes");
        }
        Ok(v)
    }
}
impl Schema {
    fn write_schema(&self, s: &mut String) {
        match self {
            Self::Null => s.push('N'),
            Self::Bool => s.push('B'),
            Self::Int => s.push('I'),
            Self::Double => s.push('D'),
            Self::Decimal => s.push('M'),
            Self::String(n) => {
                s.push('S');
                if let Some(n) = n {
                    s.push_str(&n.to_string());
                }
            }
            Self::Bytes(n) => {
                s.push('X');
                if let Some(n) = n {
                    s.push_str(&n.to_string());
                }
            }
            Self::List(v) => {
                s.push('L');
                s.push_str(&v.len().to_string());
                for x in v {
                    x.write_schema(s);
                }
            }
            Self::Object { key_bytes, values } => {
                s.push('O');
                s.push_str(&values.len().to_string());
                if let Some(n) = key_bytes {
                    s.push('K');
                    s.push_str(&n.to_string());
                }
                for x in values {
                    x.write_schema(s);
                }
            }
            Self::Array(x) => {
                s.push('A');
                x.write_schema(s);
            }
        }
    }
}
fn put_len(out: &mut Vec<u8>, n: usize, width: u8) -> Result<()> {
    let max = match width {
        2 => u16::MAX as u64,
        4 => u32::MAX as u64,
        8 => u64::MAX,
        _ => return err("bad width"),
    };
    if n as u64 > max {
        return err("length exceeds width");
    }
    for i in 0..width {
        out.push(((n as u64 >> (8 * i)) & 255) as u8);
    }
    Ok(())
}
fn encode_value(s: &Schema, v: &Value, w: u8, out: &mut Vec<u8>, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return err("value nesting limit");
    }
    match (s, v) {
        (Schema::Null, Value::Null) => {}
        (Schema::Bool, Value::Bool(b)) => out.push(if *b { b'T' } else { b'F' }),
        (Schema::Int, Value::Int(n)) => out.extend(n.to_le_bytes()),
        (Schema::Double, Value::Double(n)) if n.is_finite() => {
            out.extend(n.to_bits().to_le_bytes())
        }
        (Schema::Decimal, Value::Decimal(t)) if number_text(t) => {
            put_len(out, t.len(), w)?;
            out.extend(t.as_bytes());
        }
        (Schema::String(size), Value::String(t)) => {
            if let Some(n) = size {
                if t.len() != *n {
                    return err("fixed string length mismatch");
                }
            } else {
                put_len(out, t.len(), w)?;
            }
            out.extend(t.as_bytes());
        }
        (Schema::Bytes(size), Value::Bytes(b)) => {
            if let Some(n) = size {
                if b.len() != *n {
                    return err("fixed bytes length mismatch");
                }
            } else {
                put_len(out, b.len(), w)?;
            }
            out.extend(b);
        }
        (Schema::List(ss), Value::List(vs)) if ss.len() == vs.len() => {
            for (a, b) in ss.iter().zip(vs) {
                encode_value(a, b, w, out, depth + 1)?;
            }
        }
        (Schema::Object { key_bytes, values }, Value::Object(entries))
            if values.len() == entries.len() =>
        {
            let mut seen = HashSet::new();
            for ((key, value), schema) in entries.iter().zip(values) {
                if !seen.insert(key) {
                    return err("duplicate key");
                }
                if let Some(n) = key_bytes {
                    if key.len() != *n {
                        return err("fixed key length mismatch");
                    }
                } else {
                    put_len(out, key.len(), w)?;
                }
                out.extend(key.as_bytes());
                encode_value(schema, value, w, out, depth + 1)?;
            }
        }
        (Schema::Array(schema), Value::Array(vs)) => {
            if vs.len() > MAX_ITEMS {
                return err("array too large");
            }
            put_len(out, vs.len(), w)?;
            for v in vs {
                encode_value(schema, v, w, out, depth + 1)?;
            }
        }
        _ => return err("value does not match schema"),
    }
    if out.len() > MAX_BYTES {
        return err("data too large");
    }
    Ok(())
}
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    width: u8,
}
impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.pos.checked_add(n).ok_or("length overflow")?;
        if end > self.data.len() {
            return err("truncated data");
        }
        let a = &self.data[self.pos..end];
        self.pos = end;
        Ok(a)
    }
    fn len(&mut self) -> Result<usize> {
        let width = self.width as usize;
        let mut n = 0u64;
        for (i, &b) in self.bytes(width)?.iter().enumerate() {
            n |= (b as u64) << (8 * i);
        }
        usize::try_from(n).map_err(|_| "length overflow".into())
    }
}
fn decode_value(s: &Schema, r: &mut Reader, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return err("value nesting limit");
    }
    Ok(match s {
        Schema::Null => Value::Null,
        Schema::Bool => match r.bytes(1)?[0] {
            b'T' => Value::Bool(true),
            b'F' => Value::Bool(false),
            _ => return err("invalid bool"),
        },
        Schema::Int => Value::Int(i64::from_le_bytes(r.bytes(8)?.try_into().unwrap())),
        Schema::Double => {
            let n = f64::from_bits(u64::from_le_bytes(r.bytes(8)?.try_into().unwrap()));
            if !n.is_finite() {
                return err("non-finite double");
            }
            Value::Double(n)
        }
        Schema::Decimal => {
            let n = r.len()?;
            if n == 0 || n > MAX_BYTES {
                return err("bad decimal length");
            }
            let t = std::str::from_utf8(r.bytes(n)?).map_err(|_| "invalid decimal ASCII")?;
            if !t.is_ascii() || !number_text(t) {
                return err("invalid decimal");
            }
            Value::Decimal(t.into())
        }
        Schema::String(size) => {
            let n = match size {
                Some(n) => *n,
                None => r.len()?,
            };
            if n > MAX_BYTES {
                return err("string too large");
            }
            Value::String(
                std::str::from_utf8(r.bytes(n)?)
                    .map_err(|_| "invalid UTF-8")?
                    .into(),
            )
        }
        Schema::Bytes(size) => {
            let n = match size {
                Some(n) => *n,
                None => r.len()?,
            };
            if n > MAX_BYTES {
                return err("bytes too large");
            }
            Value::Bytes(r.bytes(n)?.to_vec())
        }
        Schema::List(schemas) => {
            let mut v = Vec::new();
            for s in schemas {
                v.push(decode_value(s, r, depth + 1)?);
            }
            Value::List(v)
        }
        Schema::Object { key_bytes, values } => {
            let mut v = Vec::new();
            let mut seen = HashSet::new();
            for s in values {
                let n = match key_bytes {
                    Some(n) => *n,
                    None => r.len()?,
                };
                if n > MAX_BYTES {
                    return err("key too large");
                }
                let key = std::str::from_utf8(r.bytes(n)?)
                    .map_err(|_| "invalid UTF-8 key")?
                    .to_string();
                if !seen.insert(key.clone()) {
                    return err("duplicate key");
                }
                v.push((key, decode_value(s, r, depth + 1)?));
            }
            Value::Object(v)
        }
        Schema::Array(schema) => {
            let n = r.len()?;
            if n > MAX_ITEMS {
                return err("array too large");
            }
            let mut v = Vec::new();
            for _ in 0..n {
                v.push(decode_value(schema, r, depth + 1)?);
            }
            Value::Array(v)
        }
    })
}
fn json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 32 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
fn double_exact(x: f64) -> Result<String> {
    if !x.is_finite() {
        return err("non-finite double");
    }
    let bits = x.to_bits();
    let neg = bits >> 63 != 0;
    let exp = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    if exp == 0 && frac == 0 {
        return Ok(if neg { "-0" } else { "0" }.into());
    }
    let mant = if exp == 0 { frac } else { frac | (1u64 << 52) };
    let power = if exp == 0 { -1074 } else { exp - 1023 - 52 };
    let mut digits = if power >= 0 {
        (BigUint::from(mant) << power as usize).to_string()
    } else {
        (BigUint::from(mant) * BigUint::from(5u8).pow((-power) as u32)).to_string()
    };
    if power < 0 {
        let places = (-power) as usize;
        if digits.len() <= places {
            digits = format!("0.{}{}", "0".repeat(places - digits.len()), digits);
        } else {
            digits.insert(digits.len() - places, '.');
        }
        while digits.ends_with('0') {
            digits.pop();
        }
        if digits.ends_with('.') {
            digits.pop();
        }
    }
    if neg {
        digits.insert(0, '-');
    }
    Ok(digits)
}
pub fn to_json(value: &Value) -> Result<String> {
    fn go(v: &Value, out: &mut String, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return err("value nesting limit");
        }
        match v {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(n) => out.push_str(&n.to_string()),
            Value::Double(n) => out.push_str(&double_exact(*n)?),
            Value::Decimal(s) => {
                if !number_text(s) {
                    return err("invalid decimal");
                }
                out.push_str(s)
            }
            Value::String(s) => json_string(s, out),
            Value::Bytes(b) => json_string(&STANDARD.encode(b), out),
            Value::List(vs) | Value::Array(vs) => {
                out.push('[');
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    go(v, out, depth + 1)?;
                }
                out.push(']');
            }
            Value::Object(vs) => {
                out.push('{');
                let mut seen = HashSet::new();
                for (i, (k, v)) in vs.iter().enumerate() {
                    if !seen.insert(k) {
                        return err("duplicate key");
                    }
                    if i > 0 {
                        out.push(',');
                    }
                    json_string(k, out);
                    out.push(':');
                    go(v, out, depth + 1)?;
                }
                out.push('}');
            }
        }
        Ok(())
    }
    let mut s = String::new();
    go(value, &mut s, 0)?;
    Ok(s)
}
pub fn pack(inner: &Document, data: &[u8]) -> Result<Vec<u8>> {
    let schema = inner.text()?;
    let outer = Document::parse("@38:L2SX")?;
    outer.encode(&Value::List(vec![
        Value::String(schema),
        Value::Bytes(data.to_vec()),
    ]))
}
pub fn unpack(data: &[u8]) -> Result<(Document, Vec<u8>)> {
    let outer = Document::parse("@38:L2SX")?;
    let Value::List(mut items) = outer.decode(data)? else {
        return err("bad envelope");
    };
    let Value::Bytes(bytes) = items.pop().unwrap() else {
        return err("bad envelope");
    };
    let Value::String(schema) = items.pop().unwrap() else {
        return err("bad envelope");
    };
    let doc = Document::parse(&schema)?;
    doc.decode(&bytes)?;
    Ok((doc, bytes))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vectors() {
        let d = Document::parse("@32:AN").unwrap();
        assert_eq!(
            d.decode(&[3, 0]).unwrap(),
            Value::Array(vec![Value::Null; 3])
        );
        assert_eq!(
            d.encode(&Value::Array(vec![Value::Null; 3])).unwrap(),
            vec![3, 0]
        );
        let d = Document::parse("@32:X3").unwrap();
        assert_eq!(
            d.decode(&[0, 255, 66]).unwrap(),
            Value::Bytes(vec![0, 255, 66])
        );
        assert_eq!(
            to_json(&Value::Bytes(vec![0, 255, 66])).unwrap(),
            "\"AP9C\""
        );
        let d = Document::parse("@32:O1K3I").unwrap();
        let v = d.decode(&[97, 98, 99, 1, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(to_json(&v).unwrap(), "{\"abc\":1}");
    }
    #[test]
    fn packed_roundtrip() {
        let d = Document::parse("@34:L2SI").unwrap();
        let v = Value::List(vec![Value::String("hello".into()), Value::Int(42)]);
        let data = d.encode(&v).unwrap();
        assert_eq!(d.decode(&data).unwrap(), v);
        let mut appended = data.clone();
        appended.push(0);
        assert!(d.decode(&appended).is_err());
        let packed = pack(&d, &data).unwrap();
        let (dd, raw) = unpack(&packed).unwrap();
        assert_eq!(dd, d);
        assert_eq!(raw, data);
    }
    #[test]
    fn shared_fixture() {
        let doc = Document::parse(include_str!("../../fixtures/full.schema").trim()).unwrap();
        let data = include_bytes!("../../fixtures/full.bin");
        let expected = include_str!("../../fixtures/full.json");
        assert_eq!(to_json(&doc.decode(data).unwrap()).unwrap(), expected);
    }
    #[test]
    fn rejects() {
        for x in ["@32:Nx", "@32:S01", "@32;compress=bzip2:N", "@32;foo=x:N"] {
            assert!(Document::parse(x).is_err(), "{x}");
        }
        assert!(Document::parse("@32:B").unwrap().decode(&[b'X']).is_err());
        assert!(Document::parse("@32:I").unwrap().decode(&[0; 7]).is_err());
    }
}
pub mod optimize;
pub use optimize::{encode_auto, EncodeOptions, Encoded};
