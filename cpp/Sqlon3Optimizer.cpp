#include "Sqlon3.h"
#include <algorithm>
#include <map>
#include <stdexcept>
#include <limits>
namespace sqlon3 {
namespace {
constexpr std::size_t MAX_DEPTH=128, MAX_ITEMS=1000000;
struct Plan { Schema schema; std::size_t bytes=0, maximum=0; };
char kind(const Value& v){return v.kind=='A'?'L':v.kind;}
std::size_t schema_size(const Schema& s){return Document{8,s}.text().size()-4;}
std::size_t count(const Value& v){return kind(v)=='O'?v.fields.size():v.items.size();}
struct Planner {
    const EncodeOptions& o;std::size_t width;
    std::map<std::vector<std::uintptr_t>,std::optional<Plan>> cache;
    std::optional<Plan> infer(const std::vector<const Value*>& vs,std::size_t depth) {
        if(depth>MAX_DEPTH)throw std::runtime_error("nesting limit");
        std::vector<std::uintptr_t> key;for(auto v:vs)key.push_back(reinterpret_cast<std::uintptr_t>(v));
        auto found=cache.find(key);if(found!=cache.end())return found->second;
        auto p=build(vs,depth);cache.emplace(key,p);return p;
    }
    std::optional<Plan> build(const std::vector<const Value*>& vs,std::size_t depth) {
        char k=kind(*vs[0]);auto n=vs.size();
        for(auto v:vs)if(kind(*v)!=k)return {};
        Schema s;s.kind=k;
        if(k=='N'||k=='B'||k=='I'||k=='D')return Plan{s,n*(k=='N'?0:k=='B'?1:8),0};
        if(k=='S'||k=='X'||k=='M') {
            auto len=[k](const Value* v){return k=='X'?v->bytes.size():v->text.size();};
            auto first=len(vs[0]);std::size_t total=0,maximum=0;bool equal=true;
            for(auto v:vs){auto l=len(v);total+=l;maximum=std::max(maximum,l);equal&=l==first;}
            bool fixed=(k=='S'?o.fixed_strings:k=='X'&&o.fixed_bytes)&&equal&&std::to_string(first).size()<width*n;
            if(fixed)s.size=first;
            return Plan{s,total+(fixed?0:width*n),fixed?0:maximum};
        }
        if(k!='L'&&k!='O')throw std::runtime_error("bad value kind");
        auto size=count(*vs[0]);bool equal=true;
        for(auto v:vs){auto l=count(*v);if(l>MAX_ITEMS)throw std::runtime_error("container too large");equal&=l==size;}
        std::optional<Plan> candidate;
        if(equal) {
            std::size_t total=0,maximum=0;bool compatible=true;
            for(std::size_t i=0;i<size;i++) {
                std::vector<const Value*> group;for(auto v:vs)group.push_back(k=='O'?&v->fields[i].second:&v->items[i]);
                auto p=infer(group,depth+1);if(!p){compatible=false;break;}
                s.children.push_back(p->schema);total+=p->bytes;maximum=std::max(maximum,p->maximum);
            }
            if(compatible) {
                if(k=='O'&&size) {
                    auto first=vs[0]->fields[0].first.size();bool same=true;std::size_t sum=0,max=0;
                    for(auto v:vs)for(auto& e:v->fields){auto l=e.first.size();sum+=l;max=std::max(max,l);same&=l==first;}
                    if(o.fixed_keys&&same&&1+std::to_string(first).size()<width*n*size)s.key_bytes=first;
                    total+=sum+(s.key_bytes?0:width*n*size);if(!s.key_bytes)maximum=std::max(maximum,max);
                }
                candidate=Plan{s,total,maximum};
            }
        }
        if(k=='L'&&o.homogeneous_arrays) {
            std::vector<const Value*> flat;std::size_t maximum=0;
            for(auto v:vs){maximum=std::max(maximum,v->items.size());for(auto& x:v->items)flat.push_back(&x);}
            if(!flat.empty()) {
                auto p=infer(flat,depth+1);
                if(p) {
                    Schema a;a.kind='A';a.children.push_back(p->schema);
                    Plan alt{a,p->bytes+width*n,std::max(p->maximum,maximum)};
                    if(!candidate||schema_size(alt.schema)+alt.bytes<schema_size(candidate->schema)+candidate->bytes)candidate=alt;
                }
            }
        }
        return candidate;
    }
};
Value adapt(const Schema& s,const Value& v) {
    if(s.kind=='L'||s.kind=='A') {
        Value out;out.kind=s.kind;
        for(std::size_t i=0;i<v.items.size();i++)out.items.push_back(adapt(s.children[s.kind=='A'?0:i],v.items[i]));
        return out;
    }
    if(s.kind=='O') {
        Value out;out.kind='O';
        for(std::size_t i=0;i<v.fields.size();i++)out.fields.emplace_back(v.fields[i].first,adapt(s.children[i],v.fields[i].second));
        return out;
    }
    return v;
}
}
Encoded encode_auto(const Value& value,const EncodeOptions& options) {
    if(options.width!=2&&options.width!=4&&options.width!=8)throw std::runtime_error("bad width");
    std::vector<int> widths=options.compact_lengths?std::vector<int>{2,4,8}:std::vector<int>{options.width};
    for(auto width:widths) {
        Planner planner{options,static_cast<std::size_t>(width),{}};
        auto p=planner.infer({&value},0);if(!p)throw std::runtime_error("cannot infer schema");
        if(width==8||static_cast<std::uint64_t>(p->maximum)<(std::uint64_t{1}<<(width*8))) {
            Document doc{width,p->schema};
            return Encoded{doc,doc.encode(adapt(doc.root,value))};
        }
    }
    throw std::runtime_error("length exceeds width");
}
}
