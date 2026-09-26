import java.util.*;
import java.nio.charset.StandardCharsets;

/** Optional size-oriented encoding; does not alter Sqlon3 decoding. */
public final class Sqlon3Optimizer {
    public record EncodeOptions(boolean fixedStrings, boolean fixedBytes, boolean homogeneousArrays,
        boolean fixedKeys, boolean compactLengths, int width) {
        public EncodeOptions() { this(false,false,false,false,false,8); }
        public static EncodeOptions optimized() { return new EncodeOptions(true,true,true,true,true,8); }
    }
    public record Encoded(Sqlon3.Document document, byte[] data) {}
    private record Plan(Sqlon3.Schema schema, long bytes, int maximum) {}
    private static char kind(Sqlon3.Value v) { return v.kind()=='A'?'L':v.kind(); }
    @SuppressWarnings("unchecked") private static List<Sqlon3.Value> items(Sqlon3.Value v) {return (List<Sqlon3.Value>)v.data();}
    @SuppressWarnings("unchecked") private static List<Sqlon3.Entry> entries(Sqlon3.Value v) {return (List<Sqlon3.Entry>)v.data();}
    private static int length(Sqlon3.Value v) {return kind(v)=='O'?entries(v).size():items(v).size();}
    private static int bytes(String s) {
        // Reject unpaired UTF-16 surrogates rather than silently replacing them.
        for(int i=0;i<s.length();i++) {
            char c=s.charAt(i);
            if(Character.isHighSurrogate(c)) {if(++i>=s.length()||!Character.isLowSurrogate(s.charAt(i)))throw new IllegalArgumentException("unpaired surrogate");}
            else if(Character.isLowSurrogate(c))throw new IllegalArgumentException("unpaired surrogate");
        }
        return s.getBytes(StandardCharsets.UTF_8).length;
    }
    private static int schemaSize(Sqlon3.Schema s) {return new Sqlon3.Document(8,s).text().length()-4;}
    private static final class Planner {
        final EncodeOptions o;final int width;
        final IdentityHashMap<Sqlon3.Value,Integer> ids=new IdentityHashMap<>();
        final Map<List<Integer>,Optional<Plan>> cache=new HashMap<>();
        Planner(EncodeOptions o,int width){this.o=o;this.width=width;}
        Plan infer(List<Sqlon3.Value> vs,int depth) {
            if(depth>Sqlon3.MAX_DEPTH)throw new IllegalArgumentException("nesting limit");
            List<Integer> key=new ArrayList<>();for(var v:vs)key.add(ids.computeIfAbsent(v,x->ids.size()));
            if(cache.containsKey(key))return cache.get(key).orElse(null);
            Plan p=build(vs,depth);cache.put(key,Optional.ofNullable(p));return p;
        }
        Plan build(List<Sqlon3.Value> vs,int depth) {
            char k=kind(vs.get(0));int n=vs.size();
            for(var v:vs)if(kind(v)!=k)return null;
            if("NBID".indexOf(k)>=0)return new Plan(new Sqlon3.Schema(k),(long)n*(k=='N'?0:k=='B'?1:8),0);
            if("SXM".indexOf(k)>=0) {
                int first=k=='X'?((byte[])vs.get(0).data()).length:bytes((String)vs.get(0).data());
                long total=0;int maximum=0;boolean equal=true;
                for(var v:vs){int len=k=='X'?((byte[])v.data()).length:bytes((String)v.data());total+=len;maximum=Math.max(maximum,len);equal&=len==first;}
                boolean fixed=(k=='S'?o.fixedStrings():k=='X'&&o.fixedBytes())&&equal&&Integer.toString(first).length()<(long)width*n;
                return new Plan(new Sqlon3.Schema(k,fixed?first:null,List.of(),null),total+(fixed?0:(long)width*n),fixed?0:maximum);
            }
            if(k!='L'&&k!='O')throw new IllegalArgumentException("bad value kind");
            int count=length(vs.get(0));boolean equal=true;
            for(var v:vs){int len=length(v);if(len>Sqlon3.MAX_ITEMS)throw new IllegalArgumentException("container too large");equal&=len==count;}
            Plan candidate=null;
            if(equal) {
                List<Sqlon3.Schema> children=new ArrayList<>();long total=0;int maximum=0;boolean compatible=true;
                for(int i=0;i<count;i++) {
                    List<Sqlon3.Value> group=new ArrayList<>();for(var v:vs)group.add(k=='O'?entries(v).get(i).value():items(v).get(i));
                    Plan p=infer(group,depth+1);if(p==null){compatible=false;break;}children.add(p.schema());total+=p.bytes();maximum=Math.max(maximum,p.maximum());
                }
                if(compatible) {
                    Integer keyBytes=null;
                    if(k=='O'&&count>0) {
                        int first=bytes(entries(vs.get(0)).get(0).key());boolean same=true;long sum=0;int max=0;
                        for(var v:vs)for(var e:entries(v)){int len=bytes(e.key());sum+=len;max=Math.max(max,len);same&=len==first;}
                        if(o.fixedKeys()&&same&&1+Integer.toString(first).length()<(long)width*n*count)keyBytes=first;
                        total+=sum+(keyBytes==null?(long)width*n*count:0);if(keyBytes==null)maximum=Math.max(maximum,max);
                    }
                    candidate=new Plan(new Sqlon3.Schema(k,null,children,keyBytes),total,maximum);
                }
            }
            if(k=='L'&&o.homogeneousArrays()) {
                List<Sqlon3.Value> flat=new ArrayList<>();int maximum=0;
                for(var v:vs){flat.addAll(items(v));maximum=Math.max(maximum,items(v).size());}
                if(!flat.isEmpty()) {
                    Plan p=infer(flat,depth+1);
                    if(p!=null) {
                        Plan alt=new Plan(new Sqlon3.Schema('A',null,List.of(p.schema()),null),p.bytes()+(long)width*n,Math.max(p.maximum(),maximum));
                        if(candidate==null||schemaSize(alt.schema())+alt.bytes()<schemaSize(candidate.schema())+candidate.bytes())candidate=alt;
                    }
                }
            }
            return candidate;
        }
    }
    private static Sqlon3.Value adapt(Sqlon3.Schema s,Sqlon3.Value v) {
        if(s.kind()=='L'||s.kind()=='A') {
            List<Sqlon3.Value> result=new ArrayList<>();int i=0;
            for(var x:items(v))result.add(adapt(s.children().get(s.kind()=='A'?0:i++),x));
            return new Sqlon3.Value(s.kind(),result);
        }
        if(s.kind()=='O') {
            List<Sqlon3.Entry> result=new ArrayList<>();int i=0;
            for(var x:entries(v))result.add(new Sqlon3.Entry(x.key(),adapt(s.children().get(i++),x.value())));
            return new Sqlon3.Value('O',result);
        }
        return v;
    }
    public static Encoded encodeAuto(Sqlon3.Value value) {return encodeAuto(value,new EncodeOptions());}
    public static Encoded encodeAuto(Sqlon3.Value value,EncodeOptions options) {
        if(options.width()!=2&&options.width()!=4&&options.width()!=8)throw new IllegalArgumentException("bad width");
        int[] widths=options.compactLengths()?new int[]{2,4,8}:new int[]{options.width()};
        for(int width:widths) {
            Plan p=new Planner(options,width).infer(List.of(value),0);
            if(p==null)throw new IllegalArgumentException("cannot infer schema");
            if(width==8||(long)p.maximum()<(1L<<(width*8))) {
                var doc=new Sqlon3.Document(width,p.schema());
                return new Encoded(doc,doc.encode(adapt(doc.root,value)));
            }
        }
        throw new IllegalArgumentException("length exceeds width");
    }
}
