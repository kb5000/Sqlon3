import java.util.*;
public final class TestOptimization {
    private static void check(boolean b){if(!b)throw new AssertionError("optimization assertion failed");}
    public static void main(String[] args) throws Exception {
        var rows=new ArrayList<Sqlon3.Value>();
        for(int i=0;i<10;i++)rows.add(new Sqlon3.Value('O',List.of(new Sqlon3.Entry("num",new Sqlon3.Value('I',(long)i)),new Sqlon3.Entry("txt",new Sqlon3.Value('S',"中")))));
        var v=new Sqlon3.Value('L',rows);
        var plain=Sqlon3Optimizer.encodeAuto(v);
        var opt=Sqlon3Optimizer.encodeAuto(v,Sqlon3Optimizer.EncodeOptions.optimized());
        check(plain.document().text().startsWith("@38:L10"));check(opt.document().text().equals("@32:AO2K3IS3"));
        check(Arrays.equals(opt.data(),java.nio.file.Files.readAllBytes(java.nio.file.Path.of("fixtures/optimized.bin"))));
        check(Sqlon3.toJson(opt.document().decode(opt.data())).equals(Sqlon3.toJson(v)));
        check(opt.data().length+opt.document().text().length()<plain.data().length+plain.document().text().length());
        for(int f=0;f<5;f++) {
            var o=new Sqlon3Optimizer.EncodeOptions(f==0,f==1,f==2,f==3,f==4,8);
            var e=Sqlon3Optimizer.encodeAuto(v,o);check(Sqlon3.toJson(e.document().decode(e.data())).equals(Sqlon3.toJson(v)));
        }
        var compact=new Sqlon3Optimizer.EncodeOptions(false,false,false,false,true,8);
        check(Sqlon3Optimizer.encodeAuto(new Sqlon3.Value('S',"x".repeat(65535)),compact).document().width==2);
        check(Sqlon3Optimizer.encodeAuto(new Sqlon3.Value('S',"x".repeat(65536)),compact).document().width==4);
        var longRows=new ArrayList<Sqlon3.Value>();for(int i=0;i<10;i++)longRows.add(new Sqlon3.Value('S',"x".repeat(65536)));
        check(Sqlon3Optimizer.encodeAuto(new Sqlon3.Value('L',longRows),Sqlon3Optimizer.EncodeOptions.optimized()).document().text().equals("@32:AS65536"));
        check(Sqlon3Optimizer.encodeAuto(new Sqlon3.Value('L',List.of()),Sqlon3Optimizer.EncodeOptions.optimized()).document().text().equals("@32:L0"));
        check(Sqlon3Optimizer.encodeAuto(new Sqlon3.Value('L',List.of(new Sqlon3.Value('I',1L))),Sqlon3Optimizer.EncodeOptions.optimized()).document().text().equals("@32:L1I"));
        check(Sqlon3Optimizer.encodeAuto(new Sqlon3.Value('X',new byte[]{1,2,3}),Sqlon3Optimizer.EncodeOptions.optimized()).document().text().equals("@32:X3"));
        var nested=new ArrayList<Sqlon3.Value>();
        for(int i=0;i<10;i++) {
            var array=new ArrayList<Sqlon3.Value>();for(int j=0;j<2*(1+i%3);j++)array.add(new Sqlon3.Value('S',j%2==0?"a":"bb"));
            nested.add(new Sqlon3.Value('O',List.of(new Sqlon3.Entry("num",new Sqlon3.Value('I',(long)i)),new Sqlon3.Entry("arr",new Sqlon3.Value(i%2==0?'L':'A',array)))));
        }
        var original=new Sqlon3.Value('L',nested);
        var encoded=Sqlon3Optimizer.encodeAuto(original,new Sqlon3Optimizer.EncodeOptions(true,true,true,true,true,8));
        check(encoded.document().text().equals("@32:AO2K3IAS"));
        check(Sqlon3.toJson(encoded.document().decode(encoded.data())).equals(Sqlon3.toJson(original)));
        System.out.println("Java optimization tests passed");
    }
}
