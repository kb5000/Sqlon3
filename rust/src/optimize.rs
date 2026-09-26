//! Optional size-oriented Schema inference. Explicit Document encoding is unchanged.
use crate::{Document, Result, Schema, Value, MAX_DEPTH, MAX_ITEMS};
use std::collections::HashMap;
#[derive(Clone, Debug)]
pub struct EncodeOptions {
    pub fixed_strings: bool,
    pub fixed_bytes: bool,
    pub homogeneous_arrays: bool,
    pub fixed_keys: bool,
    pub compact_lengths: bool,
    pub width: u8,
}
impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            fixed_strings: false,
            fixed_bytes: false,
            homogeneous_arrays: false,
            fixed_keys: false,
            compact_lengths: false,
            width: 8,
        }
    }
}
impl EncodeOptions {
    pub fn optimized() -> Self {
        Self {
            fixed_strings: true,
            fixed_bytes: true,
            homogeneous_arrays: true,
            fixed_keys: true,
            compact_lengths: true,
            ..Self::default()
        }
    }
}
pub struct Encoded {
    pub document: Document,
    pub data: Vec<u8>,
}
#[derive(Clone)]
struct Plan {
    schema: Schema,
    bytes: usize,
    max: usize,
}
fn schema_len(s: &Schema) -> usize {
    let mut text = String::new();
    s.write_schema(&mut text);
    text.len()
}
fn kind(v: &Value) -> char {
    match v {
        Value::Null => 'N',
        Value::Bool(_) => 'B',
        Value::Int(_) => 'I',
        Value::Double(_) => 'D',
        Value::Decimal(_) => 'M',
        Value::String(_) => 'S',
        Value::Bytes(_) => 'X',
        Value::List(_) | Value::Array(_) => 'L',
        Value::Object(_) => 'O',
    }
}
fn items(v: &Value) -> &[Value] {
    match v {
        Value::List(x) | Value::Array(x) => x,
        _ => unreachable!(),
    }
}
struct Planner<'a> {
    options: &'a EncodeOptions,
    width: usize,
    cache: HashMap<Vec<usize>, Option<Plan>>,
}
impl Planner<'_> {
    fn infer(&mut self, vs: &[&Value], depth: usize) -> Result<Option<Plan>> {
        if depth > MAX_DEPTH {
            return Err("nesting limit".into());
        }
        let key: Vec<_> = vs.iter().map(|v| *v as *const Value as usize).collect();
        if let Some(p) = self.cache.get(&key) {
            return Ok(p.clone());
        }
        let p = self.build(vs, depth)?;
        self.cache.insert(key, p.clone());
        Ok(p)
    }
    fn build(&mut self, vs: &[&Value], depth: usize) -> Result<Option<Plan>> {
        let k = kind(vs[0]);
        let n = vs.len();
        let w = self.width;
        if vs.iter().any(|v| kind(v) != k) {
            return Ok(None);
        }
        let primitive = match k {
            'N' => Some((Schema::Null, 0)),
            'B' => Some((Schema::Bool, 1)),
            'I' => Some((Schema::Int, 8)),
            'D' => Some((Schema::Double, 8)),
            _ => None,
        };
        if let Some((schema, size)) = primitive {
            return Ok(Some(Plan {
                schema,
                bytes: n * size,
                max: 0,
            }));
        }
        if matches!(k, 'S' | 'X' | 'M') {
            let lengths: Vec<_> = vs
                .iter()
                .map(|v| match v {
                    Value::String(t) | Value::Decimal(t) => t.len(),
                    Value::Bytes(b) => b.len(),
                    _ => unreachable!(),
                })
                .collect();
            let fixed = if k == 'S' {
                self.options.fixed_strings
            } else {
                k == 'X' && self.options.fixed_bytes
            };
            let size = if fixed
                && lengths.iter().all(|x| *x == lengths[0])
                && lengths[0].to_string().len() < w * n
            {
                Some(lengths[0])
            } else {
                None
            };
            let schema = match k {
                'S' => Schema::String(size),
                'X' => Schema::Bytes(size),
                _ => Schema::Decimal,
            };
            return Ok(Some(Plan {
                schema,
                bytes: lengths.iter().sum::<usize>() + if size.is_none() { w * n } else { 0 },
                max: if size.is_none() {
                    *lengths.iter().max().unwrap()
                } else {
                    0
                },
            }));
        }
        let lengths: Vec<_> = vs
            .iter()
            .map(|v| match v {
                Value::Object(x) => x.len(),
                _ => items(v).len(),
            })
            .collect();
        if lengths.iter().any(|n| *n > MAX_ITEMS) {
            return Err("container too large".into());
        }
        let mut candidate = None;
        if lengths.iter().all(|x| *x == lengths[0]) {
            let mut children = Vec::new();
            let mut bytes = 0;
            let mut max = 0;
            let mut compatible = true;
            for i in 0..lengths[0] {
                let group: Vec<_> = vs
                    .iter()
                    .map(|v| match v {
                        Value::Object(x) => &x[i].1,
                        _ => &items(v)[i],
                    })
                    .collect();
                if let Some(p) = self.infer(&group, depth + 1)? {
                    children.push(p.schema);
                    bytes += p.bytes;
                    max = max.max(p.max);
                } else {
                    compatible = false;
                    break;
                }
            }
            if compatible {
                let schema = if k == 'O' {
                    let keys: Vec<_> = vs
                        .iter()
                        .flat_map(|v| match v {
                            Value::Object(x) => {
                                x.iter().map(|(key, _)| key.len()).collect::<Vec<_>>()
                            }
                            _ => unreachable!(),
                        })
                        .collect();
                    let key_bytes = if !keys.is_empty()
                        && self.options.fixed_keys
                        && keys.iter().all(|x| *x == keys[0])
                        && 1 + keys[0].to_string().len() < w * keys.len()
                    {
                        Some(keys[0])
                    } else {
                        None
                    };
                    bytes += keys.iter().sum::<usize>()
                        + if key_bytes.is_none() {
                            w * keys.len()
                        } else {
                            0
                        };
                    if key_bytes.is_none() {
                        max = max.max(*keys.iter().max().unwrap_or(&0));
                    }
                    Schema::Object {
                        key_bytes,
                        values: children,
                    }
                } else {
                    Schema::List(children)
                };
                candidate = Some(Plan { schema, bytes, max });
            }
        }
        if k == 'L' && self.options.homogeneous_arrays {
            let flat: Vec<_> = vs.iter().flat_map(|v| items(v).iter()).collect();
            if !flat.is_empty() {
                if let Some(p) = self.infer(&flat, depth + 1)? {
                    let alt = Plan {
                        schema: Schema::Array(Box::new(p.schema)),
                        bytes: p.bytes + w * n,
                        max: p.max.max(*lengths.iter().max().unwrap()),
                    };
                    if candidate.as_ref().map_or(true, |c| {
                        schema_len(&alt.schema) + alt.bytes < schema_len(&c.schema) + c.bytes
                    }) {
                        candidate = Some(alt);
                    }
                }
            }
        }
        Ok(candidate)
    }
}
fn adapt(s: &Schema, v: &Value) -> Value {
    match (s, v) {
        (Schema::List(ss), Value::List(vs) | Value::Array(vs)) => {
            Value::List(ss.iter().zip(vs).map(|(s, v)| adapt(s, v)).collect())
        }
        (Schema::Array(s), Value::List(vs) | Value::Array(vs)) => {
            Value::Array(vs.iter().map(|v| adapt(s, v)).collect())
        }
        (Schema::Object { values, .. }, Value::Object(vs)) => Value::Object(
            values
                .iter()
                .zip(vs)
                .map(|(s, (k, v))| (k.clone(), adapt(s, v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}
pub fn encode_auto(value: &Value, options: &EncodeOptions) -> Result<Encoded> {
    if ![2, 4, 8].contains(&options.width) {
        return Err("bad width".into());
    }
    let widths = if options.compact_lengths {
        vec![2, 4, 8]
    } else {
        vec![options.width]
    };
    for width in widths {
        let mut planner = Planner {
            options,
            width: width as usize,
            cache: HashMap::new(),
        };
        let p = planner.infer(&[value], 0)?.ok_or("cannot infer schema")?;
        if width == 8 || (p.max as u64) < (1u64 << (width * 8)) {
            let document = Document {
                width,
                root: p.schema,
            };
            let data = document.encode(&adapt(&document.root, value))?;
            return Ok(Encoded { document, data });
        }
    }
    Err("length exceeds width".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_json;
    #[test]
    fn optimization_and_flags() {
        let v = Value::List(
            (0..10)
                .map(|i| {
                    Value::Object(vec![
                        ("num".into(), Value::Int(i)),
                        ("txt".into(), Value::String("中".into())),
                    ])
                })
                .collect(),
        );
        let plain = encode_auto(&v, &EncodeOptions::default()).unwrap();
        let optimized = encode_auto(&v, &EncodeOptions::optimized()).unwrap();
        assert!(plain.document.text().unwrap().starts_with("@38:L10"));
        assert_eq!(optimized.document.text().unwrap(), "@32:AO2K3IS3");
        assert_eq!(
            optimized.data,
            include_bytes!("../../fixtures/optimized.bin")
        );
        assert_eq!(
            to_json(&optimized.document.decode(&optimized.data).unwrap()).unwrap(),
            to_json(&v).unwrap()
        );
        assert!(
            optimized.document.text().unwrap().len() + optimized.data.len()
                < plain.document.text().unwrap().len() + plain.data.len()
        );
        for flag in 0..5 {
            let mut o = EncodeOptions::default();
            match flag {
                0 => o.fixed_strings = true,
                1 => o.fixed_bytes = true,
                2 => o.homogeneous_arrays = true,
                3 => o.fixed_keys = true,
                _ => o.compact_lengths = true,
            };
            let e = encode_auto(&v, &o).unwrap();
            assert_eq!(
                to_json(&e.document.decode(&e.data).unwrap()).unwrap(),
                to_json(&v).unwrap()
            );
        }
    }
    #[test]
    fn nested_common_schema() {
        let v = Value::List(
            (0..10)
                .map(|i| {
                    Value::Object(vec![
                        ("num".into(), Value::Int(i)),
                        (
                            "arr".into(),
                            Value::List(
                                (0..2 * (1 + i % 3))
                                    .map(|j| {
                                        Value::String(if j % 2 == 0 { "a" } else { "bb" }.into())
                                    })
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        );
        let o = EncodeOptions::optimized();
        let e = encode_auto(&v, &o).unwrap();
        assert_eq!(e.document.text().unwrap(), "@32:AO2K3IAS");
        assert_eq!(
            crate::to_json(&e.document.decode(&e.data).unwrap()).unwrap(),
            crate::to_json(&v).unwrap()
        );
    }
    #[test]
    fn boundaries_and_fallback() {
        let o = EncodeOptions {
            compact_lengths: true,
            ..EncodeOptions::default()
        };
        for (n, w) in [(65535, 2), (65536, 4)] {
            assert_eq!(
                encode_auto(&Value::String("x".repeat(n)), &o)
                    .unwrap()
                    .document
                    .width,
                w
            );
        }
        let v = Value::List(vec![Value::String("x".repeat(65536)); 10]);
        assert_eq!(
            encode_auto(&v, &EncodeOptions::optimized())
                .unwrap()
                .document
                .text()
                .unwrap(),
            "@32:AS65536"
        );
        for (v, t) in [
            (Value::List(vec![]), "@32:L0"),
            (Value::List(vec![Value::Int(1)]), "@32:L1I"),
            (Value::Bytes(b"abc".to_vec()), "@32:X3"),
        ] {
            assert_eq!(
                encode_auto(&v, &EncodeOptions::optimized())
                    .unwrap()
                    .document
                    .text()
                    .unwrap(),
                t
            );
        }
    }
}
