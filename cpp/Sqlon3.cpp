#include "Sqlon3.h"
#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <set>
#include <stdexcept>
#include <string_view>

namespace sqlon3 {
namespace {
constexpr std::size_t MAX_BYTES=64u*1024u*1024u, MAX_ITEMS=1000000, MAX_DEPTH=128;
[[noreturn]] void bad(const char* m){throw std::runtime_error(m);}
bool digit(char c){return c>='0'&&c<='9';}
bool number_text(std::string_view s){
    std::size_t i=0;if(i<s.size()&&s[i]=='-')++i;
    if(i==s.size())return false;
    if(s[i]=='0')++i;
    else if(s[i]>='1'&&s[i]<='9')while(i<s.size()&&digit(s[i]))++i;
    else return false;
    if(i<s.size()&&s[i]=='.'){++i;auto start=i;while(i<s.size()&&digit(s[i]))++i;if(i==start)return false;}
    if(i<s.size()&&(s[i]=='e'||s[i]=='E')){
        ++i;if(i<s.size()&&(s[i]=='+'||s[i]=='-'))++i;
        auto start=i;while(i<s.size()&&digit(s[i]))++i;if(i==start)return false;
    }
    return i==s.size();
}
void utf8(const std::string& s){
    const auto* b=reinterpret_cast<const unsigned char*>(s.data());
    for(std::size_t i=0;i<s.size();){
        unsigned c=b[i];
        if(c<128){++i;continue;}
        int n=c>=0xC2&&c<=0xDF?2:c>=0xE0&&c<=0xEF?3:c>=0xF0&&c<=0xF4?4:0;
        if(!n||i+n>s.size())bad("invalid UTF-8");
        for(int j=1;j<n;j++)if((b[i+j]&0xC0)!=0x80)bad("invalid UTF-8");
        if(n==3 && ((c==0xE0&&b[i+1]<0xA0)||(c==0xED&&b[i+1]>=0xA0)))bad("invalid UTF-8");
        if(n==4 && ((c==0xF0&&b[i+1]<0x90)||(c==0xF4&&b[i+1]>=0x90)))bad("invalid UTF-8");
        i+=n;
    }
}
struct Parser{
    const std::string& s;std::size_t pos=0;
    std::optional<std::size_t> count(bool required){
        auto start=pos;while(pos<s.size()&&digit(s[pos]))++pos;
        if(start==pos){if(required)bad("missing schema count");return std::nullopt;}
        if(pos-start>1&&s[start]=='0')bad("leading zero");
        std::size_t n=0;
        for(auto i=start;i<pos;i++){
            auto d=static_cast<std::size_t>(s[i]-'0');
            if(n>(MAX_BYTES-d)/10)bad("schema count limit");
            n=n*10+d;
        }
        return n;
    }
    Schema value(std::size_t depth=0){
        if(depth>MAX_DEPTH||pos>=s.size())bad("invalid schema tree");
        Schema v;v.kind=s[pos++];
        switch(v.kind){
            case 'N':case 'B':case 'I':case 'D':case 'M':return v;
            case 'S':case 'X':v.size=count(false);return v;
            case 'A':v.children.push_back(value(depth+1));return v;
            case 'L':case 'O':{
                auto n=*count(true);if(n>MAX_ITEMS)bad("container too large");
                if(v.kind=='O'&&pos<s.size()&&s[pos]=='K'){++pos;v.key_bytes=count(true);}
                for(std::size_t i=0;i<n;i++)v.children.push_back(value(depth+1));
                return v;
            }
            default:bad("unknown schema code");
        }
    }
};
void schema_text(const Schema& s,std::string& out){
    out.push_back(s.kind);
    if(s.kind=='S'||s.kind=='X'){if(s.size)out+=std::to_string(*s.size);}
    else if(s.kind=='A')schema_text(s.children.at(0),out);
    else if(s.kind=='L'||s.kind=='O'){
        out+=std::to_string(s.children.size());
        if(s.kind=='O'&&s.key_bytes){out.push_back('K');out+=std::to_string(*s.key_bytes);}
        for(const auto& c:s.children)schema_text(c,out);
    }
}
void put_len(std::vector<std::uint8_t>& out,std::size_t n,int width){
    if(width!=2&&width!=4&&width!=8)bad("bad width");
    if(width<8 && n>=(std::size_t{1}<<(width*8)))bad("length exceeds width");
    for(int i=0;i<width;i++)out.push_back(static_cast<std::uint8_t>((n>>(8*i))&255));
}
void append(std::vector<std::uint8_t>& out,const std::string& s){
    out.insert(out.end(),s.begin(),s.end());
}
void encode_value(const Schema& s,const Value& v,int width,std::vector<std::uint8_t>& out,std::size_t depth){
    if(depth>MAX_DEPTH||s.kind!=v.kind)bad("value does not match schema");
    switch(s.kind){
        case 'N':break;
        case 'B':out.push_back(v.boolean?'T':'F');break;
        case 'I':{
            auto n=static_cast<std::uint64_t>(v.integer);
            for(int i=0;i<8;i++)out.push_back(static_cast<std::uint8_t>(n>>(8*i)));break;
        }
        case 'D':{
            if(!std::isfinite(v.real))bad("non-finite double");
            std::uint64_t n;std::memcpy(&n,&v.real,8);
            for(int i=0;i<8;i++)out.push_back(static_cast<std::uint8_t>(n>>(8*i)));break;
        }
        case 'M':{
            if(!number_text(v.text))bad("invalid decimal");
            put_len(out,v.text.size(),width);append(out,v.text);break;
        }
        case 'S':{
            utf8(v.text);
            if(s.size){if(v.text.size()!=*s.size)bad("fixed string length mismatch");}
            else put_len(out,v.text.size(),width);
            append(out,v.text);break;
        }
        case 'X':{
            if(s.size){if(v.bytes.size()!=*s.size)bad("fixed bytes length mismatch");}
            else put_len(out,v.bytes.size(),width);
            out.insert(out.end(),v.bytes.begin(),v.bytes.end());break;
        }
        case 'L':{
            if(v.items.size()!=s.children.size())bad("list length mismatch");
            for(std::size_t i=0;i<v.items.size();i++)encode_value(s.children[i],v.items[i],width,out,depth+1);
            break;
        }
        case 'O':{
            if(v.fields.size()!=s.children.size())bad("object length mismatch");
            std::set<std::string> seen;
            for(std::size_t i=0;i<v.fields.size();i++){
                const auto& [key,item]=v.fields[i];utf8(key);
                if(!seen.insert(key).second)bad("duplicate key");
                if(s.key_bytes){if(key.size()!=*s.key_bytes)bad("fixed key length mismatch");}
                else put_len(out,key.size(),width);
                append(out,key);encode_value(s.children[i],item,width,out,depth+1);
            }
            break;
        }
        case 'A':{
            if(v.items.size()>MAX_ITEMS)bad("array too large");
            put_len(out,v.items.size(),width);
            for(const auto& item:v.items)encode_value(s.children.at(0),item,width,out,depth+1);
            break;
        }
        default:bad("bad schema");
    }
    if(out.size()>MAX_BYTES)bad("data too large");
}
struct Reader{
    const std::vector<std::uint8_t>& data;int width;std::size_t pos=0;
    std::string bytes(std::size_t n){
        if(n>data.size()-pos)bad("truncated data");
        std::string s(data.begin()+pos,data.begin()+pos+n);pos+=n;return s;
    }
    std::size_t length(){
        auto raw=bytes(width);std::uint64_t n=0;
        for(int i=0;i<width;i++)n|=static_cast<std::uint64_t>(static_cast<std::uint8_t>(raw[i]))<<(8*i);
        if(n>std::numeric_limits<std::size_t>::max())bad("length overflow");
        return static_cast<std::size_t>(n);
    }
};
Value decode_value(const Schema& s,Reader& r,std::size_t depth){
    if(depth>MAX_DEPTH)bad("nesting limit");
    Value v;v.kind=s.kind;
    switch(s.kind){
        case 'N':return v;
        case 'B':{
            auto x=r.bytes(1)[0];if(x!='T'&&x!='F')bad("invalid bool");
            v.boolean=x=='T';return v;
        }
        case 'I':{
            auto raw=r.bytes(8);std::uint64_t n=0;
            for(int i=0;i<8;i++)n|=static_cast<std::uint64_t>(static_cast<std::uint8_t>(raw[i]))<<(8*i);
            std::memcpy(&v.integer,&n,8);return v;
        }
        case 'D':{
            auto raw=r.bytes(8);std::uint64_t n=0;
            for(int i=0;i<8;i++)n|=static_cast<std::uint64_t>(static_cast<std::uint8_t>(raw[i]))<<(8*i);
            std::memcpy(&v.real,&n,8);if(!std::isfinite(v.real))bad("non-finite double");return v;
        }
        case 'M':{
            auto n=r.length();if(n==0||n>MAX_BYTES)bad("bad decimal length");
            v.text=r.bytes(n);if(!number_text(v.text))bad("invalid decimal");return v;
        }
        case 'S':{
            auto n=s.size ? *s.size : r.length();
            if(n>MAX_BYTES)bad("string too large");
            v.text=r.bytes(n);utf8(v.text);return v;
        }
        case 'X':{
            auto n=s.size ? *s.size : r.length();
            if(n>MAX_BYTES)bad("bytes too large");
            auto raw=r.bytes(n);v.bytes.assign(raw.begin(),raw.end());return v;
        }
        case 'L':{
            for(const auto& c:s.children)v.items.push_back(decode_value(c,r,depth+1));return v;
        }
        case 'O':{
            std::set<std::string> seen;
            for(const auto& c:s.children){
                auto n=s.key_bytes ? *s.key_bytes : r.length();
                if(n>MAX_BYTES)bad("key too large");
                auto key=r.bytes(n);utf8(key);
                if(!seen.insert(key).second)bad("duplicate key");
                v.fields.emplace_back(key,decode_value(c,r,depth+1));
            }
            return v;
        }
        case 'A':{
            auto n=r.length();if(n>MAX_ITEMS)bad("array too large");
            for(std::size_t i=0;i<n;i++)v.items.push_back(decode_value(s.children.at(0),r,depth+1));
            return v;
        }
        default:bad("bad schema");
    }
}
std::string json_string(const std::string& s){
    utf8(s);std::string out="\"";static const char hex[]="0123456789abcdef";
    for(unsigned char c:s){
        switch(c){
            case '"':out+="\\\"";break;
            case '\\':out+="\\\\";break;
            case 8:out+="\\b";break;
            case 9:out+="\\t";break;
            case 10:out+="\\n";break;
            case 12:out+="\\f";break;
            case 13:out+="\\r";break;
            default:
                if(c<32){out+="\\u00";out.push_back(hex[c>>4]);out.push_back(hex[c&15]);}
                else out.push_back(static_cast<char>(c));
        }
    }
    out.push_back('"');return out;
}
std::string base64(const std::vector<std::uint8_t>& bytes){
    static const char abc[]="ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    for(std::size_t i=0;i<bytes.size();i+=3){
        unsigned a=bytes[i], b=i+1<bytes.size()?bytes[i+1]:0, c=i+2<bytes.size()?bytes[i+2]:0;
        out.push_back(abc[a>>2]);out.push_back(abc[((a&3)<<4)|(b>>4)]);
        out.push_back(i+1<bytes.size()?abc[((b&15)<<2)|(c>>6)]:'=');
        out.push_back(i+2<bytes.size()?abc[c&63]:'=');
    }
    return out;
}
void multiply(std::string& s,int n){
    int carry=0;
    for(auto i=s.rbegin();i!=s.rend();++i){
        int x=(*i-'0')*n+carry;*i=static_cast<char>('0'+x%10);carry=x/10;
    }
    while(carry){s.insert(s.begin(),static_cast<char>('0'+carry%10));carry/=10;}
}
std::string exact_double(double x){
    if(!std::isfinite(x))bad("non-finite double");
    std::uint64_t bits;std::memcpy(&bits,&x,8);
    bool neg=(bits>>63)!=0;int exponent=static_cast<int>((bits>>52)&2047);
    std::uint64_t frac=bits&((std::uint64_t{1}<<52)-1);
    if(exponent==0&&frac==0)return neg?"-0":"0";
    auto mant=exponent==0?frac:frac|(std::uint64_t{1}<<52);
    int power=exponent==0?-1074:exponent-1023-52;
    std::string digits=std::to_string(mant);
    for(int i=0;i<std::abs(power);i++)multiply(digits,power>=0?2:5);
    if(power<0){
        auto places=static_cast<std::size_t>(-power);
        if(digits.size()<=places)digits=std::string(places-digits.size()+1,'0')+digits;
        digits.insert(digits.size()-places,1,'.');
        while(digits.back()=='0')digits.pop_back();
        if(digits.back()=='.')digits.pop_back();
    }
    if(neg)digits.insert(digits.begin(),'-');return digits;
}
std::string json_value(const Value& v,std::size_t depth){
    if(depth>MAX_DEPTH)bad("nesting limit");
    switch(v.kind){
        case 'N':return "null";
        case 'B':return v.boolean?"true":"false";
        case 'I':return std::to_string(v.integer);
        case 'D':return exact_double(v.real);
        case 'M':if(!number_text(v.text))bad("invalid decimal");return v.text;
        case 'S':return json_string(v.text);
        case 'X':return json_string(base64(v.bytes));
        case 'L':case 'A':{
            std::string out="[";
            for(std::size_t i=0;i<v.items.size();i++){if(i)out.push_back(',');out+=json_value(v.items[i],depth+1);}
            return out+"]";
        }
        case 'O':{
            std::string out="{";std::set<std::string> seen;
            for(std::size_t i=0;i<v.fields.size();i++){
                const auto& [key,item]=v.fields[i];
                if(!seen.insert(key).second)bad("duplicate key");
                if(i)out.push_back(',');
                out+=json_string(key)+":"+json_value(item,depth+1);
            }
            return out+"}";
        }
        default:bad("bad value");
    }
}

}
Document Document::parse(const std::string& text){
    for(unsigned char c:text)if(c>127)bad("schema must be ASCII");
    auto colon=text.find(':');if(colon==std::string::npos)bad("missing header colon");
    auto head=text.substr(0,colon);
    if(head.size()<3||head.substr(0,2)!="@3")bad("unsupported version");
    int width=head[2]-'0';if(width!=2&&width!=4&&width!=8)bad("bad width");
    auto tail=head.substr(3);
    if(!tail.empty()){
        if(tail[0]!=';')bad("bad header");
        std::set<std::string> seen;std::size_t start=1;
        while(start<=tail.size()){
            auto end=tail.find(';',start);auto part=tail.substr(start,end==std::string::npos?end:end-start);
            auto eq=part.find('=');if(eq==std::string::npos)bad("bad extension");
            auto name=part.substr(0,eq),value=part.substr(eq+1);
            if(name.empty()||!(name[0]>='a'&&name[0]<='z')||value.empty())bad("bad extension");
            for(char c:name)if(!(c>='a'&&c<='z')&&!digit(c)&&c!='_')bad("bad extension");
            for(char c:value)if(!(c>='a'&&c<='z')&&!(c>='A'&&c<='Z')&&!digit(c)&&c!='.'&&c!='_'&&c!='-')bad("bad extension");
            if(!seen.insert(name).second)bad("duplicate extension");
            bad("unknown extension");
            if(end==std::string::npos)break;start=end+1;
        }
    }
    auto body=text.substr(colon+1);Parser p{body};
    auto root=p.value();if(p.pos!=body.size())bad("trailing schema bytes");
    return {width,root};
}
std::string Document::text()const{
    if(width!=2&&width!=4&&width!=8)bad("bad width");
    std::string out="@3"+std::to_string(width);
    out.push_back(':');schema_text(root,out);return out;
}
std::vector<std::uint8_t> Document::encode(const Value& value)const{
    std::vector<std::uint8_t> raw;encode_value(root,value,width,raw,0);
    return raw;
}
Value Document::decode(const std::vector<std::uint8_t>& data)const{
    if(data.size()>MAX_BYTES)bad("data too large");
    const auto& raw=data;
    Reader r{raw,width};auto v=decode_value(root,r,0);
    if(r.pos!=raw.size())bad("trailing data");return v;
}
std::string to_json(const Value& value){return json_value(value,0);}
std::vector<std::uint8_t> pack(const Document& doc,const std::vector<std::uint8_t>& data){
    Value v;v.kind='L';Value schema;schema.kind='S';schema.text=doc.text();
    Value bytes;bytes.kind='X';bytes.bytes=data;v.items={schema,bytes};
    return Document::parse("@38:L2SX").encode(v);
}
std::pair<Document,std::vector<std::uint8_t>> unpack(const std::vector<std::uint8_t>& data){
    auto v=Document::parse("@38:L2SX").decode(data);
    auto doc=Document::parse(v.items.at(0).text);
    auto bytes=v.items.at(1).bytes;doc.decode(bytes);return {doc,bytes};
}
}
