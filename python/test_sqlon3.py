import unittest
import sqlon3 as s


class Sqlon3Tests(unittest.TestCase):
    def test_vectors(self):
        d = s.Document.parse("@32:AN")
        self.assertEqual(s.to_json(d.decode(bytes([3, 0]))), "[null,null,null]")
        self.assertEqual(d.encode(s.Value("A", [s.Value("N")] * 3)), bytes([3, 0]))
        x = s.Document.parse("@32:X3")
        self.assertEqual(s.to_json(x.decode(bytes([0, 255, 66]))), '"AP9C"')
        obj = s.Document.parse("@32:O1K3I")
        self.assertEqual(s.to_json(obj.decode(b"abc" + (1).to_bytes(8, "little"))), '{"abc":1}')

    def test_pack(self):
        d = s.Document.parse("@34:L2SI")
        v = s.Value("L", [s.Value("S", "hello"), s.Value("I", 42)])
        raw = d.encode(v)
        self.assertEqual(d.decode(raw), v)
        self.assertEqual(s.unpack(s.pack(d, raw)), (d, raw))

    def test_shared_fixture(self):
        from pathlib import Path
        root = Path(__file__).resolve().parents[1] / "fixtures"
        doc = s.Document.parse((root / "full.schema").read_text(encoding="ascii"))
        data = (root / "full.bin").read_bytes()
        expected = (root / "full.json").read_text(encoding="utf-8")
        self.assertEqual(s.to_json(doc.decode(data)), expected)
    def test_invalid(self):
        for text in ("@32:Nx", "@32:S01", "@32;compress=bzip2:N", "@32;foo=x:N"):
            with self.assertRaises(ValueError):
                s.Document.parse(text)
        with self.assertRaises(ValueError):
            s.Document.parse("@32:B").decode(b"X")
        with self.assertRaises(ValueError):
            s.Document.parse("@32:I").decode(bytes(7))



class OptimizationTests(unittest.TestCase):
    def rows(self):
        return s.Value("L",[s.Value("O",[("num",s.Value("I",i)),("txt",s.Value("S","中"))]) for i in range(10)])
    def test_options_and_roundtrip(self):
        v=self.rows()
        plain=s.encode_auto(v)
        optimized=s.encode_auto(v,s.EncodeOptions.optimized())
        self.assertTrue(plain.document.text().startswith("@38:L10"))
        self.assertEqual(optimized.document.text(),"@32:AO2K3IS3")
        from pathlib import Path
        self.assertEqual(optimized.data,(Path(__file__).resolve().parents[1]/"fixtures/optimized.bin").read_bytes())
        self.assertEqual(s.to_json(optimized.document.decode(optimized.data)),s.to_json(v))
        self.assertLess(len(optimized.document.text())+len(optimized.data),len(plain.document.text())+len(plain.data))
        for flag in ("fixed_strings","fixed_bytes","homogeneous_arrays","fixed_keys","compact_lengths"):
            result=s.encode_auto(v,s.EncodeOptions(**{flag:True}))
            self.assertEqual(s.to_json(result.document.decode(result.data)),s.to_json(v))
    def test_boundaries_and_fallback(self):
        options=s.EncodeOptions(compact_lengths=True)
        for n,w in ((65535,2),(65536,4)):
            self.assertEqual(s.encode_auto(s.Value("S","x"*n),options).document.width,w)
        v=s.Value("L",[s.Value("S","x"*65536)]*10)
        self.assertEqual(s.encode_auto(v,s.EncodeOptions.optimized()).document.text(),"@32:AS65536")
        for v,text in ((s.Value("L",[]),"@32:L0"),(s.Value("L",[s.Value("I",1)]),"@32:L1I"),(s.Value("X",b"abc"),"@32:X3")):
            self.assertEqual(s.encode_auto(v,s.EncodeOptions.optimized()).document.text(),text)
        v=s.Value("L",[s.Value("I",1),s.Value("S","a")])
        result=s.encode_auto(v,s.EncodeOptions.optimized())
        self.assertEqual(result.document.root.kind,"L")
        self.assertEqual(s.to_json(result.document.decode(result.data)),s.to_json(v))

    def test_nested_common_schema(self):
        v=s.Value("L",[s.Value("O",[("num",s.Value("I",i)),("arr",s.Value("A" if i%2 else "L",[s.Value("S","a"),s.Value("S","bb")]*(1+i%3)))]) for i in range(10)])
        e=s.encode_auto(v,s.EncodeOptions.optimized())
        self.assertEqual(e.document.text(),"@32:AO2K3IAS")
        self.assertEqual(s.to_json(e.document.decode(e.data)),s.to_json(v))


if __name__ == "__main__":
    unittest.main()
