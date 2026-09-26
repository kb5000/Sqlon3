import test from 'node:test';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { Document, value, toJson, pack, unpack } from './sqlon3.js';

test('vectors', () => {
  const d = Document.parse('@32:AN');
  assert.equal(toJson(d.decode(Buffer.from([3, 0]))), '[null,null,null]');
  assert.deepEqual(d.encode(value('A', [value('N'), value('N'), value('N')])), Buffer.from([3, 0]));
  assert.equal(toJson(Document.parse('@32:X3').decode(Buffer.from([0, 255, 66]))), '"AP9C"');
  assert.equal(toJson(Document.parse('@32:O1K3I').decode(Buffer.from([97,98,99,1,0,0,0,0,0,0,0]))), '{"abc":1}');
});
test('packing', () => {
  const d = Document.parse('@34:L2SI');
  const v = value('L', [value('S', 'hello'), value('I', 42n)]);
  const raw = d.encode(v);
  assert.equal(toJson(d.decode(raw)), '["hello",42]');
  const [dd, bytes] = unpack(pack(d, raw));
  assert.equal(dd.text(), d.text());
  assert.deepEqual(bytes, raw);
});
test('rejects invalid schema and data', () => {
  for (const text of ['@32:Nx', '@32:S01', '@32;compress=bzip2:N', '@32;foo=x:N'])
    assert.throws(() => Document.parse(text));
  assert.throws(() => Document.parse('@32:B').decode(Buffer.from('X')));
});

test('shared cross-language fixture', () => {
  const root = new URL('../fixtures/', import.meta.url);
  const schema = readFileSync(new URL('full.schema', root), 'ascii').trim();
  const data = readFileSync(new URL('full.bin', root));
  const expected = readFileSync(new URL('full.json', root), 'utf8');
  assert.equal(toJson(Document.parse(schema).decode(data)), expected);
});

import { EncodeOptions, encodeAuto } from './sqlon3.js';
test('optimization and individual flags',()=>{
  const v=value('L',Array.from({length:10},(_,i)=>value('O',[['num',value('I',BigInt(i))],['txt',value('S','中')]])));
  const plain=encodeAuto(v), optimized=encodeAuto(v,EncodeOptions.optimized());
  assert.ok(plain.document.text().startsWith('@38:L10'));
  assert.equal(optimized.document.text(),'@32:AO2K3IS3');
  assert.deepEqual(optimized.data,readFileSync(new URL('../fixtures/optimized.bin',import.meta.url)));
  assert.equal(toJson(optimized.document.decode(optimized.data)),toJson(v));
  assert.ok(optimized.document.text().length+optimized.data.length<plain.document.text().length+plain.data.length);
  for(const flag of ['fixedStrings','fixedBytes','homogeneousArrays','fixedKeys','compactLengths']) {
    const e=encodeAuto(v,new EncodeOptions({[flag]:true}));assert.equal(toJson(e.document.decode(e.data)),toJson(v));
  }
});
test('optimization boundaries and fallback',()=>{
  for(const [n,w] of [[65535,2],[65536,4]])assert.equal(encodeAuto(value('S','x'.repeat(n)),{compactLengths:true}).document.width,w);
  assert.equal(encodeAuto(value('L',Array.from({length:10},()=>value('S','x'.repeat(65536)))),EncodeOptions.optimized()).document.text(),'@32:AS65536');
  for(const [v,t] of [[value('L',[]),'@32:L0'],[value('L',[value('I',1n)]),'@32:L1I'],[value('X',Buffer.from('abc')),'@32:X3']])assert.equal(encodeAuto(v,EncodeOptions.optimized()).document.text(),t);
});

test('nested common schema with varying array and string lengths',()=>{
  const v=value('L',Array.from({length:10},(_,i)=>value('O',[['num',value('I',BigInt(i))],['arr',value(i%2?'A':'L',Array.from({length:2*(1+i%3)},(_,j)=>value('S',j%2?'bb':'a')))]])));
  const e=encodeAuto(v,EncodeOptions.optimized());
  assert.equal(e.document.text(),'@32:AO2K3IAS');
  assert.equal(toJson(e.document.decode(e.data)),toJson(v));
});
