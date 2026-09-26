const MAX_BYTES = 64 * 1024 * 1024;
const MAX_ITEMS = 1_000_000;
const MAX_DEPTH = 128;
const enc = new TextEncoder();
const dec = new TextDecoder('utf-8', { fatal: true });
const numberRE = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?$/;
const fail = m => { throw new Error(m); };
export const value = (kind, data = null) => ({ kind, data });

function count(p, required) {
  const start = p.pos;
  while (p.pos < p.text.length && /[0-9]/.test(p.text[p.pos])) p.pos++;
  if (start === p.pos) return required ? fail('missing schema count') : null;
  const text = p.text.slice(start, p.pos);
  if (text.length > 1 && text[0] === '0') fail('leading zero');
  const n = Number(text);
  if (!Number.isSafeInteger(n) || n > MAX_BYTES) fail('schema count overflow');
  return n;
}
function parseValue(p, depth = 0) {
  if (depth > MAX_DEPTH || p.pos >= p.text.length) fail('invalid schema tree');
  const kind = p.text[p.pos++];
  if ('NBIDM'.includes(kind)) return { kind };
  if ('SX'.includes(kind)) return { kind, size: count(p, false) };
  if (kind === 'A') return { kind, children: [parseValue(p, depth + 1)] };
  if (kind === 'L' || kind === 'O') {
    const n = count(p, true);
    if (n > MAX_ITEMS) fail('container too large');
    let keyBytes = null;
    if (kind === 'O' && p.text[p.pos] === 'K') { p.pos++; keyBytes = count(p, true); }
    const children = [];
    for (let i = 0; i < n; i++) children.push(parseValue(p, depth + 1));
    return { kind, children, keyBytes };
  }
  fail('unknown schema code');
}
function schemaText(s) {
  if ('NBIDM'.includes(s.kind)) return s.kind;
  if ('SX'.includes(s.kind)) return s.kind + (s.size === null ? '' : s.size);
  if (s.kind === 'A') return 'A' + schemaText(s.children[0]);
  if (s.kind === 'L' || s.kind === 'O')
    return s.kind + s.children.length + (s.kind === 'O' && s.keyBytes !== null ? 'K' + s.keyBytes : '')
      + s.children.map(schemaText).join('');
  fail('bad schema');
}
function scalarText(text) {
  if (typeof text !== 'string') fail('expected string');
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) fail('unpaired surrogate');
    } else if (c >= 0xdc00 && c <= 0xdfff) fail('unpaired surrogate');
  }
  return Buffer.from(enc.encode(text));
}
function putLen(chunks, n, width) {
  n = BigInt(n);
  if (n < 0n || n >= (1n << BigInt(width * 8))) fail('length exceeds width');
  const b = Buffer.alloc(width);
  for (let i = 0; i < width; i++) b[i] = Number((n >> BigInt(i * 8)) & 255n);
  chunks.push(b);
}
function encodeValue(s, v, width, chunks, depth = 0) {
  if (depth > MAX_DEPTH || !v || s.kind !== v.kind) fail('value does not match schema');
  const x = v.data;
  switch (s.kind) {
    case 'N': break;
    case 'B': chunks.push(Buffer.from([x ? 84 : 70])); break;
    case 'I': {
      const b = Buffer.alloc(8); b.writeBigInt64LE(BigInt(x)); chunks.push(b); break;
    }
    case 'D': {
      if (!Number.isFinite(x)) fail('non-finite double');
      const b = Buffer.alloc(8); b.writeDoubleLE(x); chunks.push(b); break;
    }
    case 'M': {
      if (typeof x !== 'string' || !numberRE.test(x)) fail('invalid decimal');
      const b = Buffer.from(x, 'ascii'); putLen(chunks, b.length, width); chunks.push(b); break;
    }
    case 'S': case 'X': {
      const b = s.kind === 'S' ? scalarText(x) : Buffer.from(x);
      if (s.size === null) putLen(chunks, b.length, width);
      else if (b.length !== s.size) fail('fixed length mismatch');
      chunks.push(b); break;
    }
    case 'L': {
      if (!Array.isArray(x) || x.length !== s.children.length) fail('list length mismatch');
      s.children.forEach((child, i) => encodeValue(child, x[i], width, chunks, depth + 1)); break;
    }
    case 'O': {
      if (!Array.isArray(x) || x.length !== s.children.length) fail('object length mismatch');
      const seen = new Set();
      x.forEach(([key, item], i) => {
        if (seen.has(key)) fail('duplicate key'); seen.add(key);
        const b = scalarText(key);
        if (s.keyBytes === null) putLen(chunks, b.length, width);
        else if (b.length !== s.keyBytes) fail('fixed key length mismatch');
        chunks.push(b); encodeValue(s.children[i], item, width, chunks, depth + 1);
      }); break;
    }
    case 'A': {
      if (!Array.isArray(x) || x.length > MAX_ITEMS) fail('array too large');
      putLen(chunks, x.length, width);
      x.forEach(item => encodeValue(s.children[0], item, width, chunks, depth + 1)); break;
    }
    default: fail('bad schema');
  }
}
class Reader {
  constructor(data, width) { this.data = Buffer.from(data); this.width = width; this.pos = 0; }
  take(n) {
    if (!Number.isSafeInteger(n) || n < 0 || n > this.data.length - this.pos) fail('truncated data');
    const b = this.data.subarray(this.pos, this.pos + n); this.pos += n; return b;
  }
  length() {
    const b = this.take(this.width); let n = 0n;
    for (let i = 0; i < b.length; i++) n |= BigInt(b[i]) << BigInt(i * 8);
    if (n > BigInt(Number.MAX_SAFE_INTEGER)) fail('length overflow');
    return Number(n);
  }
}
function decodeValue(s, r, depth = 0) {
  if (depth > MAX_DEPTH) fail('nesting limit');
  switch (s.kind) {
    case 'N': return value('N');
    case 'B': {
      const b = r.take(1)[0];
      if (b !== 84 && b !== 70) fail('invalid bool');
      return value('B', b === 84);
    }
    case 'I': return value('I', r.take(8).readBigInt64LE());
    case 'D': {
      const n = r.take(8).readDoubleLE();
      if (!Number.isFinite(n)) fail('non-finite double');
      return value('D', n);
    }
    case 'M': {
      const n = r.length();
      if (n === 0 || n > MAX_BYTES) fail('bad decimal length');
      const b = r.take(n);
      if (b.some(x => x > 127)) fail('invalid decimal ASCII');
      const t = b.toString('ascii');
      if (!numberRE.test(t)) fail('invalid decimal');
      return value('M', t);
    }
    case 'S': case 'X': {
      const n = s.size === null ? r.length() : s.size;
      if (n > MAX_BYTES) fail('value too large');
      const b = r.take(n);
      return value(s.kind, s.kind === 'S' ? dec.decode(b) : Buffer.from(b));
    }
    case 'L': return value('L', s.children.map(x => decodeValue(x, r, depth + 1)));
    case 'O': {
      const seen = new Set(), items = [];
      for (const child of s.children) {
        const n = s.keyBytes === null ? r.length() : s.keyBytes;
        if (n > MAX_BYTES) fail('key too large');
        const key = dec.decode(r.take(n));
        if (seen.has(key)) fail('duplicate key');
        seen.add(key); items.push([key, decodeValue(child, r, depth + 1)]);
      }
      return value('O', items);
    }
    case 'A': {
      const n = r.length(); if (n > MAX_ITEMS) fail('array too large');
      const items = [];
      for (let i = 0; i < n; i++) items.push(decodeValue(s.children[0], r, depth + 1));
      return value('A', items);
    }
    default: fail('bad schema');
  }
}
export class Document {
  constructor(width, root) { this.width = width; this.root = root; }
  static parse(text) {
    if (typeof text !== 'string' || /[^\x00-\x7f]/.test(text)) fail('schema must be ASCII');
    const colon = text.indexOf(':'); if (colon < 0) fail('missing header colon');
    const head = text.slice(0, colon);
    if (!head.startsWith('@3') || head.length < 3) fail('unsupported version');
    const width = Number(head[2]); if (![2, 4, 8].includes(width)) fail('bad width');
    const tail = head.slice(3);
    if (tail) {
      if (!tail.startsWith(';')) fail('bad header');
      const seen = new Set();
      for (const part of tail.slice(1).split(';')) {
        const eq = part.indexOf('='); if (eq < 0) fail('bad extension');
        const name = part.slice(0, eq), val = part.slice(eq + 1);
        if (!/^[a-z][a-z0-9_]*$/.test(name) || !/^[A-Za-z0-9._-]+$/.test(val) || seen.has(name)) fail('bad extension');
        seen.add(name);
        fail('unknown extension');
      }
    }
    const p = { text: text.slice(colon + 1), pos: 0 };
    const root = parseValue(p);
    if (p.pos !== p.text.length) fail('trailing schema bytes');
    return new Document(width, root);
  }
  text() { return '@3' + this.width + ':' + schemaText(this.root); }
  encode(v) {
    const chunks = []; encodeValue(this.root, v, this.width, chunks);
    const data = Buffer.concat(chunks);
    if (data.length > MAX_BYTES) fail('data too large');
    return data;
  }
  decode(data) {
    if (data.length > MAX_BYTES) fail('data too large');
    const raw = Buffer.from(data);
    const r = new Reader(raw, this.width);
    const v = decodeValue(this.root, r);
    if (r.pos !== raw.length) fail('trailing data');
    return v;
  }
}
function jsonString(text) {
  scalarText(text);
  return JSON.stringify(text);
}
function doubleExact(n) {
  if (!Number.isFinite(n)) fail('non-finite double');
  if (Object.is(n, -0)) return '-0';
  if (n === 0) return '0';
  const b = Buffer.alloc(8); b.writeDoubleLE(n);
  const bits = b.readBigUInt64LE();
  const sign = (bits >> 63n) !== 0n ? '-' : '';
  const exp = Number((bits >> 52n) & 2047n);
  const frac = bits & ((1n << 52n) - 1n);
  const mant = exp === 0 ? frac : frac | (1n << 52n);
  const power = exp === 0 ? -1074 : exp - 1023 - 52;
  let digits = power >= 0 ? (mant << BigInt(power)).toString() : (mant * 5n ** BigInt(-power)).toString();
  if (power < 0) {
    const places = -power;
    digits = digits.padStart(places + 1, '0');
    digits = digits.slice(0, -places) + '.' + digits.slice(-places);
    digits = digits.replace(/0+$/, '').replace(/\.$/, '');
  }
  return sign + digits;
}
export function toJson(v) {
  function go(x, depth) {
    if (depth > MAX_DEPTH) fail('nesting limit');
    switch (x.kind) {
      case 'N': return 'null';
      case 'B': return x.data ? 'true' : 'false';
      case 'I': return BigInt(x.data).toString();
      case 'D': return doubleExact(x.data);
      case 'M': if (!numberRE.test(x.data)) fail('invalid decimal'); return x.data;
      case 'S': return jsonString(x.data);
      case 'X': return jsonString(Buffer.from(x.data).toString('base64'));
      case 'L': case 'A': return '[' + x.data.map(y => go(y, depth + 1)).join(',') + ']';
      case 'O': {
        const seen = new Set();
        return '{' + x.data.map(([key, item]) => {
          if (seen.has(key)) fail('duplicate key');
          seen.add(key); return jsonString(key) + ':' + go(item, depth + 1);
        }).join(',') + '}';
      }
      default: fail('bad value');
    }
  }
  return go(v, 0);
}
export function pack(doc, data) {
  return Document.parse('@38:L2SX').encode(value('L', [value('S', doc.text()), value('X', data)]));
}
export function unpack(data) {
  const [schema, raw] = Document.parse('@38:L2SX').decode(data).data;
  const doc = Document.parse(schema.data); doc.decode(raw.data);
  return [doc, raw.data];
}

export class EncodeOptions {
  constructor(options = {}) {
    Object.assign(this, { fixedStrings:false, fixedBytes:false, homogeneousArrays:false,
      fixedKeys:false, compactLengths:false, width:8 }, options);
  }
  static optimized(options = {}) {
    return new EncodeOptions({ fixedStrings:true, fixedBytes:true, homogeneousArrays:true,
      fixedKeys:true, compactLengths:true, ...options });
  }
}

export function encodeAuto(v, options = new EncodeOptions()) {
  const o = new EncodeOptions(options);
  if (![2,4,8].includes(o.width)) fail('bad width');
  const schemaSize = s => new Document(8,s).text().length - 4;
  function plan(width) {
    const ids=new WeakMap(); let next=0; const cache=new Map();
    function infer(vs,depth=0) {
      if(depth>MAX_DEPTH) fail('nesting limit');
      const key=vs.map(v=>{if(!ids.has(v))ids.set(v,++next);return ids.get(v);}).join(',');
      if(cache.has(key))return cache.get(key);
      const p=build(vs,depth);cache.set(key,p);return p;
    }
    function build(vs,depth) {
      const logical=v=>'LA'.includes(v.kind)?'L':v.kind;
      const k=logical(vs[0]),n=vs.length;
      if(vs.some(v=>logical(v)!==k))return null;
      const schema=(kind,size=null,children=[],keyBytes=null)=>({kind,size,children,keyBytes});
      if('NBID'.includes(k))return {s:schema(k),bytes:n*({N:0,B:1,I:8,D:8}[k]),max:0};
      if('SXM'.includes(k)) {
        const lengths=vs.map(v=>k==='X'?v.data.length:scalarText(v.data).length);
        const fixed=k!=='M'&&(k==='S'?o.fixedStrings:o.fixedBytes);
        const size=fixed&&lengths.every(x=>x===lengths[0])&&String(lengths[0]).length<width*n?lengths[0]:null;
        return {s:schema(k,size),bytes:lengths.reduce((a,b)=>a+b,0)+(size===null?width*n:0),max:size===null?lengths.reduce((a,b)=>Math.max(a,b),0):0};
      }
      if(!'LO'.includes(k))fail('bad value kind');
      const rows=vs.map(v=>v.data);
      if(rows.some(r=>r.length>MAX_ITEMS))fail('container too large');
      let candidate=null;
      if(rows.every(r=>r.length===rows[0].length)) {
        const children=[];let bytes=0,max=0,compatible=true;
        for(let i=0;i<rows[0].length;i++) {
          const p=infer(rows.map(r=>k==='O'?r[i][1]:r[i]),depth+1);
          if(!p){compatible=false;break;}children.push(p.s);bytes+=p.bytes;max=Math.max(max,p.max);
        }
        if(compatible) {
          let keyBytes=null;
          if(k==='O') {
            const lengths=rows.flatMap(r=>r.map(([key])=>scalarText(key).length));
            if(lengths.length) {
              if(o.fixedKeys&&lengths.every(x=>x===lengths[0])&&1+String(lengths[0]).length<width*lengths.length)keyBytes=lengths[0];
              bytes+=lengths.reduce((a,b)=>a+b,0)+(keyBytes===null?width*lengths.length:0);
              if(keyBytes===null)max=Math.max(max,lengths.reduce((a,b)=>Math.max(a,b),0));
            }
          }
          candidate={s:schema(k,null,children,keyBytes),bytes,max};
        }
      }
      if(k==='L'&&o.homogeneousArrays) {
        const flat=rows.flat();
        if(flat.length) {
          const p=infer(flat,depth+1);
          if(p) {
            const alt={s:schema('A',null,[p.s]),bytes:p.bytes+width*n,max:Math.max(p.max,rows.reduce((a,r)=>Math.max(a,r.length),0))};
            if(!candidate||schemaSize(alt.s)+alt.bytes<schemaSize(candidate.s)+candidate.bytes)candidate=alt;
          }
        }
      }
      return candidate;
    }
    return infer([v]);
  }
  let doc;
  for(const width of o.compactLengths?[2,4,8]:[o.width]) {
    const p=plan(width);if(!p)fail('cannot infer schema');
    if(BigInt(p.max)<1n<<BigInt(width*8)){doc=new Document(width,p.s);break;}
  }
  if(!doc)fail('length exceeds width');
  function adapt(s,v) {
    if('LA'.includes(s.kind))return value(s.kind,v.data.map((x,i)=>adapt(s.kind==='A'?s.children[0]:s.children[i],x)));
    if(s.kind==='O')return value('O',v.data.map(([key,x],i)=>[key,adapt(s.children[i],x)]));
    return v;
  }
  return { document:doc, data:doc.encode(adapt(doc.root,v)) };
}
