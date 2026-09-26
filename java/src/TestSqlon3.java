import java.util.*;
import java.nio.file.*;
public final class TestSqlon3 {
    static void check(boolean ok){if(!ok)throw new AssertionError();}
    public static void main(String[] args) throws Exception {
        var fixtureRoot=Path.of("fixtures");
        var fixtureDoc=Sqlon3.Document.parse(Files.readString(fixtureRoot.resolve("full.schema")).trim());
        check(Sqlon3.toJson(fixtureDoc.decode(Files.readAllBytes(fixtureRoot.resolve("full.bin")))).equals(Files.readString(fixtureRoot.resolve("full.json"))));
        var nulls=Sqlon3.Document.parse("@32:AN");
        check(Sqlon3.toJson(new Sqlon3.Value('D',0.1)).equals("0.1000000000000000055511151231257827021181583404541015625"));
        check(Sqlon3.toJson(nulls.decode(new byte[]{3,0})).equals("[null,null,null]"));
        var bytes=Sqlon3.Document.parse("@32:X3");
        check(Sqlon3.toJson(bytes.decode(new byte[]{0,(byte)255,66})).equals("\"AP9C\""));
        var object=Sqlon3.Document.parse("@32:O1K3I");
        check(Sqlon3.toJson(object.decode(new byte[]{97,98,99,1,0,0,0,0,0,0,0})).equals("{\"abc\":1}"));
        var z=Sqlon3.Document.parse("@34:L2SI");
        var v=new Sqlon3.Value('L',List.of(new Sqlon3.Value('S',"hello"),new Sqlon3.Value('I',42L)));
        byte[] raw=z.encode(v);
        check(Sqlon3.toJson(z.decode(raw)).equals("[\"hello\",42]"));
        byte[] appended=Arrays.copyOf(raw,raw.length+1);
        try{z.decode(appended);throw new AssertionError("accepted trailing byte");}catch(IllegalArgumentException expected){}
        check(Sqlon3.unpack(Sqlon3.pack(z,raw)).document().text().equals(z.text()));
        for(String s:List.of("@32:Nx","@32:S01","@32;compress=bzip2:N","@32;foo=x:N")){
            try{Sqlon3.Document.parse(s);throw new AssertionError();}catch(IllegalArgumentException expected){}
        }
        System.out.println("Java Sqlon 3 tests passed");
    }
}
