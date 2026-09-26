#include "Sqlon3.h"
#include <cassert>
#include <iostream>
#include <fstream>
#include <iterator>
#include <stdexcept>
using namespace sqlon3;
int main(){
    std::ifstream sf("fixtures/full.schema"), df("fixtures/full.bin",std::ios::binary), jf("fixtures/full.json");
    std::string schema((std::istreambuf_iterator<char>(sf)),{}), expected((std::istreambuf_iterator<char>(jf)),{});
    std::vector<std::uint8_t> fixture((std::istreambuf_iterator<char>(df)),{});
    assert(to_json(Document::parse(schema).decode(fixture))==expected);
    auto nulls=Document::parse("@32:AN");
    Value dv;dv.kind='D';dv.real=0.1;assert(to_json(dv)=="0.1000000000000000055511151231257827021181583404541015625");
    assert(to_json(nulls.decode({3,0}))=="[null,null,null]");
    Value nv;nv.kind='N';Value array;array.kind='A';array.items={nv,nv,nv};
    assert(nulls.encode(array)==std::vector<std::uint8_t>({3,0}));
    auto bytes=Document::parse("@32:X3");
    assert(to_json(bytes.decode({0,255,66}))=="\"AP9C\"");
    auto object=Document::parse("@32:O1K3I");
    assert(to_json(object.decode({97,98,99,1,0,0,0,0,0,0,0}))=="{\"abc\":1}");
    auto z=Document::parse("@34:L2SI");
    Value sv;sv.kind='S';sv.text="hello";Value iv;iv.kind='I';iv.integer=42;
    Value list;list.kind='L';list.items={sv,iv};
    auto raw=z.encode(list);assert(to_json(z.decode(raw))=="[\"hello\",42]");
    auto appended=raw;appended.push_back(0);try{z.decode(appended);assert(false);}catch(const std::runtime_error&){}
    auto packed=unpack(pack(z,raw));assert(packed.first.text()==z.text());assert(packed.second==raw);
    try{Document::parse("@32:S01");assert(false);}catch(const std::runtime_error&){}
    try{Document::parse("@32;compress=bzip2:N");assert(false);}catch(const std::runtime_error&){}
    std::cout<<"C++ Sqlon 3 tests passed\n";
}
