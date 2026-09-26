import java.io.*;
import java.math.BigInteger;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.CharBuffer;
import java.nio.charset.*;
import java.util.*;
import java.util.regex.Pattern;

public final class Sqlon3 {
    public static final int MAX_BYTES = 64 * 1024 * 1024;
    public static final int MAX_ITEMS = 1_000_000;
    public static final int MAX_DEPTH = 128;
    private static final Pattern NUMBER = Pattern.compile("-?(0|[1-9][0-9]*)(\\.[0-9]+)?([eE][+-]?[0-9]+)?");
    private static final Pattern NAME = Pattern.compile("[a-z][a-z0-9_]*");
    private static final Pattern EXT_VALUE = Pattern.compile("[A-Za-z0-9._-]+");

    public record Schema(char kind, Integer size, List<Schema> children, Integer keyBytes) {
        public Schema(char kind) { this(kind, null, List.of(), null); }
    }
    public record Value(char kind, Object data) {
        public Value(char kind) { this(kind, null); }
    }
    public record Entry(String key, Value value) {}

    private static IllegalArgumentException bad(String message) { return new IllegalArgumentException(message); }
    private static byte[] utf8(String text) {
        try {
            ByteBuffer b = StandardCharsets.UTF_8.newEncoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .encode(CharBuffer.wrap(text));
            byte[] raw = new byte[b.remaining()]; b.get(raw); return raw;
        } catch (CharacterCodingException e) { throw bad("invalid Unicode scalar"); }
    }
    private static String fromUtf8(byte[] bytes) {
        try {
            return StandardCharsets.UTF_8.newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(bytes)).toString();
        } catch (CharacterCodingException e) { throw bad("invalid UTF-8"); }
    }
    private static final class Parser {
        final String text; int pos;
        Parser(String text) { this.text=text; }
        Integer count(boolean required) {
            int start=pos;
            while (pos<text.length() && text.charAt(pos)>='0' && text.charAt(pos)<='9') pos++;
            if (start==pos) { if(required) throw bad("missing schema count"); return null; }
            if (pos-start>1 && text.charAt(start)=='0') throw bad("leading zero");
            try {
                long n=Long.parseLong(text.substring(start,pos));
                if(n>MAX_BYTES) throw bad("schema count limit");
                return (int)n;
            } catch(NumberFormatException e) { throw bad("schema count overflow"); }
        }
        Schema value(int depth) {
            if(depth>MAX_DEPTH || pos>=text.length()) throw bad("invalid schema tree");
            char k=text.charAt(pos++);
            if("NBIDM".indexOf(k)>=0) return new Schema(k);
            if(k=='S'||k=='X') return new Schema(k,count(false),List.of(),null);
            if(k=='A') return new Schema(k,null,List.of(value(depth+1)),null);
            if(k=='L'||k=='O') {
                int n=count(true);
                if(n>MAX_ITEMS) throw bad("container too large");
                Integer key=null;
                if(k=='O'&&pos<text.length()&&text.charAt(pos)=='K') {pos++;key=count(true);}
                List<Schema> children=new ArrayList<>();
                for(int i=0;i<n;i++) children.add(value(depth+1));
                return new Schema(k,null,children,key);
            }
            throw bad("unknown schema code");
        }
    }
    public static final class Document {
        public final int width; public final Schema root;
        public Document(int width, Schema root) {
            if(width!=2&&width!=4&&width!=8) throw bad("bad width");
            this.width=width;this.root=root;
        }
        public static Document parse(String text) {
            if(!text.chars().allMatch(c->c<128)) throw bad("schema must be ASCII");
            int colon=text.indexOf(':');
            if(colon<0) throw bad("missing header colon");
            String head=text.substring(0,colon);
            if(!head.startsWith("@3")||head.length()<3) throw bad("unsupported version");
            int width=head.charAt(2)-'0';
            if(width!=2&&width!=4&&width!=8) throw bad("bad width");
            String tail=head.substring(3);
            if(!tail.isEmpty()) {
                if(!tail.startsWith(";")) throw bad("bad header");
                Set<String> names=new HashSet<>();
                for(String part:tail.substring(1).split(";",-1)) {
                    int eq=part.indexOf('=');
                    if(eq<0) throw bad("bad extension");
                    String name=part.substring(0,eq), value=part.substring(eq+1);
                    if(!NAME.matcher(name).matches()||!EXT_VALUE.matcher(value).matches()||!names.add(name))
                        throw bad("bad extension");
                    throw bad("unknown extension");
                }
            }
            Parser p=new Parser(text.substring(colon+1));
            Schema root=p.value(0);
            if(p.pos!=p.text.length()) throw bad("trailing schema bytes");
            return new Document(width,root);
        }
        public String text() {
            StringBuilder b=new StringBuilder("@3").append(width);
            b.append(':');schemaText(root,b);return b.toString();
        }
        public byte[] encode(Value value) {
            ByteArrayOutputStream out=new ByteArrayOutputStream();
            encodeValue(root,value,width,out,0);
            byte[] raw=out.toByteArray();
            if(raw.length>MAX_BYTES) throw bad("data too large");
            return raw;
        }
        public Value decode(byte[] data) {
            if(data.length>MAX_BYTES) throw bad("data too large");
            byte[] raw=data;
            Reader reader=new Reader(raw,width);
            Value value=decodeValue(root,reader,0);
            if(reader.pos!=raw.length)throw bad("trailing data");
            return value;
        }
    }
    private static void schemaText(Schema s,StringBuilder b) {
        char k=s.kind();b.append(k);
        if(k=='S'||k=='X'){if(s.size()!=null)b.append(s.size());return;}
        if(k=='L'||k=='O'){
            b.append(s.children().size());
            if(k=='O'&&s.keyBytes()!=null)b.append('K').append(s.keyBytes());
            for(Schema child:s.children())schemaText(child,b);
        }else if(k=='A')schemaText(s.children().get(0),b);
    }
    private static void putLen(ByteArrayOutputStream out,long n,int width){
        if(n<0||(width<8&&n>=(1L<<(width*8))))throw bad("length exceeds width");
        for(int i=0;i<width;i++)out.write((int)(n >>> (8*i))&255);
    }
    private static void encodeValue(Schema s,Value v,int width,ByteArrayOutputStream out,int depth){
        if(depth>MAX_DEPTH||s.kind()!=v.kind())throw bad("value does not match schema");
        char k=s.kind();Object data=v.data();
        switch(k){
            case 'N':break;
            case 'B':out.write((Boolean)data?'T':'F');break;
            case 'I':{
                long n=((Number)data).longValue();
                for(int i=0;i<8;i++)out.write((int)(n >>> (8*i))&255);
                break;
            }
            case 'D':{
                double d=((Number)data).doubleValue();
                if(!Double.isFinite(d))throw bad("non-finite double");
                long n=Double.doubleToRawLongBits(d);
                for(int i=0;i<8;i++)out.write((int)(n >>> (8*i))&255);
                break;
            }
            case 'M':{
                String t=(String)data;
                if(!NUMBER.matcher(t).matches())throw bad("invalid decimal");
                byte[] raw=t.getBytes(StandardCharsets.US_ASCII);
                putLen(out,raw.length,width);out.writeBytes(raw);break;
            }
            case 'S':case 'X':{
                byte[] raw=k=='S'?utf8((String)data):(byte[])data;
                if(s.size()==null)putLen(out,raw.length,width);
                else if(raw.length!=s.size())throw bad("fixed length mismatch");
                out.writeBytes(raw);break;
            }
            case 'L':{
                List<Value> items=castList(data);
                if(items.size()!=s.children().size())throw bad("list length mismatch");
                for(int i=0;i<items.size();i++)encodeValue(s.children().get(i),items.get(i),width,out,depth+1);
                break;
            }
            case 'O':{
                List<Entry> items=castEntries(data);
                if(items.size()!=s.children().size())throw bad("object length mismatch");
                Set<String> seen=new HashSet<>();
                for(int i=0;i<items.size();i++){
                    Entry e=items.get(i);
                    if(!seen.add(e.key()))throw bad("duplicate key");
                    byte[] raw=utf8(e.key());
                    if(s.keyBytes()==null)putLen(out,raw.length,width);
                    else if(raw.length!=s.keyBytes())throw bad("fixed key length mismatch");
                    out.writeBytes(raw);
                    encodeValue(s.children().get(i),e.value(),width,out,depth+1);
                }
                break;
            }
            case 'A':{
                List<Value> items=castList(data);
                if(items.size()>MAX_ITEMS)throw bad("array too large");
                putLen(out,items.size(),width);
                for(Value item:items)encodeValue(s.children().get(0),item,width,out,depth+1);
                break;
            }
            default:throw bad("bad schema");
        }
        if(out.size()>MAX_BYTES)throw bad("data too large");
    }
    @SuppressWarnings("unchecked")
    private static List<Value> castList(Object o){return (List<Value>)o;}
    @SuppressWarnings("unchecked")
    private static List<Entry> castEntries(Object o){return (List<Entry>)o;}
    private static final class Reader {
        final byte[] data;final int width;int pos;
        Reader(byte[] data,int width){this.data=data;this.width=width;}
        byte[] take(int n){
            if(n<0||n>data.length-pos)throw bad("truncated data");
            byte[] result=Arrays.copyOfRange(data,pos,pos+n);pos+=n;return result;
        }
        int length(){
            byte[] raw=take(width);long n=0;
            for(int i=0;i<raw.length;i++)n|=(long)(raw[i]&255)<<(8*i);
            if(n<0||n>Integer.MAX_VALUE)throw bad("length overflow");
            return (int)n;
        }
    }
    private static Value decodeValue(Schema s,Reader r,int depth){
        if(depth>MAX_DEPTH)throw bad("nesting limit");
        char k=s.kind();
        switch(k){
            case 'N':return new Value('N');
            case 'B':{
                int b=r.take(1)[0]&255;
                if(b!='T'&&b!='F')throw bad("invalid bool");
                return new Value('B',b=='T');
            }
            case 'I':return new Value('I',ByteBuffer.wrap(r.take(8)).order(ByteOrder.LITTLE_ENDIAN).getLong());
            case 'D':{
                double n=ByteBuffer.wrap(r.take(8)).order(ByteOrder.LITTLE_ENDIAN).getDouble();
                if(!Double.isFinite(n))throw bad("non-finite double");
                return new Value('D',n);
            }
            case 'M':{
                int n=r.length();if(n==0||n>MAX_BYTES)throw bad("bad decimal length");
                byte[] raw=r.take(n);
                for(byte b:raw)if(b<0)throw bad("invalid decimal ASCII");
                String t=new String(raw,StandardCharsets.US_ASCII);
                if(!NUMBER.matcher(t).matches())throw bad("invalid decimal");
                return new Value('M',t);
            }
            case 'S':case 'X':{
                int n=s.size()==null?r.length():s.size();
                if(n>MAX_BYTES)throw bad("value too large");
                byte[] raw=r.take(n);
                return new Value(k,k=='S'?fromUtf8(raw):raw);
            }
            case 'L':{
                List<Value> items=new ArrayList<>();
                for(Schema child:s.children())items.add(decodeValue(child,r,depth+1));
                return new Value('L',items);
            }
            case 'O':{
                List<Entry> items=new ArrayList<>();Set<String> seen=new HashSet<>();
                for(Schema child:s.children()){
                    int n=s.keyBytes()==null?r.length():s.keyBytes();
                    if(n>MAX_BYTES)throw bad("key too large");
                    String key=fromUtf8(r.take(n));
                    if(!seen.add(key))throw bad("duplicate key");
                    items.add(new Entry(key,decodeValue(child,r,depth+1)));
                }
                return new Value('O',items);
            }
            case 'A':{
                int n=r.length();if(n>MAX_ITEMS)throw bad("array too large");
                List<Value> items=new ArrayList<>();
                for(int i=0;i<n;i++)items.add(decodeValue(s.children().get(0),r,depth+1));
                return new Value('A',items);
            }
            default:throw bad("bad schema");
        }
    }
    private static void jsonString(String s,StringBuilder out){
        utf8(s);out.append('"');
        for(int i=0;i<s.length();){
            int cp=s.codePointAt(i);i+=Character.charCount(cp);
            switch(cp){
                case '"':out.append("\\\"");break;
                case '\\':out.append("\\\\");break;
                case 8:out.append("\\b");break;
                case 9:out.append("\\t");break;
                case 10:out.append("\\n");break;
                case 12:out.append("\\f");break;
                case 13:out.append("\\r");break;
                default:
                    if(cp<32)out.append(String.format("\\u%04x",cp));
                    else out.appendCodePoint(cp);
            }
        }
        out.append('"');
    }
    private static String exactDouble(double x){
        if(!Double.isFinite(x))throw bad("non-finite double");
        long bits=Double.doubleToRawLongBits(x);boolean neg=bits<0;
        int exponent=(int)((bits>>>52)&2047);
        long fraction=bits&((1L<<52)-1);
        if(exponent==0&&fraction==0)return neg?"-0":"0";
        long mantissa=exponent==0?fraction:fraction|(1L<<52);
        int power=exponent==0?-1074:exponent-1023-52;
        String digits;
        if(power>=0)digits=BigInteger.valueOf(mantissa).shiftLeft(power).toString();
        else digits=BigInteger.valueOf(mantissa).multiply(BigInteger.valueOf(5).pow(-power)).toString();
        if(power<0){
            int places=-power;
            if(digits.length()<=places)digits="0".repeat(places-digits.length()+1)+digits;
            int cut=digits.length()-places;
            digits=digits.substring(0,cut)+"."+digits.substring(cut);
            while(digits.endsWith("0"))digits=digits.substring(0,digits.length()-1);
            if(digits.endsWith("."))digits=digits.substring(0,digits.length()-1);
        }
        return (neg?"-":"")+digits;
    }
    public static String toJson(Value value){
        StringBuilder out=new StringBuilder();jsonValue(value,out,0);return out.toString();
    }
    private static void jsonValue(Value v,StringBuilder out,int depth){
        if(depth>MAX_DEPTH)throw bad("nesting limit");
        switch(v.kind()){
            case 'N':out.append("null");break;
            case 'B':out.append((Boolean)v.data()?"true":"false");break;
            case 'I':out.append(v.data());break;
            case 'D':out.append(exactDouble((Double)v.data()));break;
            case 'M':{
                String t=(String)v.data();
                if(!NUMBER.matcher(t).matches())throw bad("invalid decimal");
                out.append(t);break;
            }
            case 'S':jsonString((String)v.data(),out);break;
            case 'X':jsonString(Base64.getEncoder().encodeToString((byte[])v.data()),out);break;
            case 'L':case 'A':{
                out.append('[');boolean first=true;
                for(Value item:castList(v.data())){if(!first)out.append(',');first=false;jsonValue(item,out,depth+1);}
                out.append(']');break;
            }
            case 'O':{
                out.append('{');boolean first=true;Set<String> seen=new HashSet<>();
                for(Entry e:castEntries(v.data())){
                    if(!seen.add(e.key()))throw bad("duplicate key");
                    if(!first)out.append(',');first=false;
                    jsonString(e.key(),out);out.append(':');jsonValue(e.value(),out,depth+1);
                }
                out.append('}');break;
            }
            default:throw bad("bad value");
        }
    }
    public record Packed(Document document,byte[] data){}
    public static byte[] pack(Document inner,byte[] data){
        Document outer=Document.parse("@38:L2SX");
        return outer.encode(new Value('L',List.of(new Value('S',inner.text()),new Value('X',data))));
    }
    public static Packed unpack(byte[] data){
        List<Value> members=castList(Document.parse("@38:L2SX").decode(data).data());
        Document inner=Document.parse((String)members.get(0).data());
        byte[] raw=(byte[])members.get(1).data();
        inner.decode(raw);return new Packed(inner,raw);
    }
}
