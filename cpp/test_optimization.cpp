#include "Sqlon3.h"
#include <stdexcept>
#include <iostream>
#include <fstream>
#include <iterator>
using namespace sqlon3;
void check(bool b){if(!b)throw std::runtime_error("optimization assertion failed");}
Value string_value(const std::string& s){Value v;v.kind='S';v.text=s;return v;}
int main() {
    Value v;v.kind='L';
    for(int i=0;i<10;i++){Value row;row.kind='O';Value number;number.kind='I';number.integer=i;row.fields={{"num",number},{"txt",string_value("\xe4\xb8\xad")}};v.items.push_back(row);}
    auto plain=encode_auto(v),opt=encode_auto(v,EncodeOptions::optimized());
    check(plain.document.text().find("@38:L10")==0);check(opt.document.text()=="@32:AO2K3IS3");
    std::ifstream fixture("fixtures/optimized.bin",std::ios::binary);std::vector<std::uint8_t> expected((std::istreambuf_iterator<char>(fixture)),{});check(opt.data==expected);
    check(to_json(opt.document.decode(opt.data))==to_json(v));
    check(opt.document.text().size()+opt.data.size()<plain.document.text().size()+plain.data.size());
    for(int f=0;f<5;f++) {
        EncodeOptions o;switch(f){case 0:o.fixed_strings=true;break;case 1:o.fixed_bytes=true;break;case 2:o.homogeneous_arrays=true;break;case 3:o.fixed_keys=true;break;case 4:o.compact_lengths=true;break;}
        auto e=encode_auto(v,o);check(to_json(e.document.decode(e.data))==to_json(v));
    }
    EncodeOptions compact;compact.compact_lengths=true;
    check(encode_auto(string_value(std::string(65535,'x')),compact).document.width==2);
    check(encode_auto(string_value(std::string(65536,'x')),compact).document.width==4);
    Value longRows;longRows.kind='L';for(int i=0;i<10;i++)longRows.items.push_back(string_value(std::string(65536,'x')));
    check(encode_auto(longRows,EncodeOptions::optimized()).document.text()=="@32:AS65536");
    Value empty;empty.kind='L';check(encode_auto(empty,EncodeOptions::optimized()).document.text()=="@32:L0");
    Value number;number.kind='I';number.integer=1;empty.items.push_back(number);check(encode_auto(empty,EncodeOptions::optimized()).document.text()=="@32:L1I");
    Value bytes;bytes.kind='X';bytes.bytes={1,2,3};check(encode_auto(bytes,EncodeOptions::optimized()).document.text()=="@32:X3");
    Value nested;nested.kind='L';
    for(int i=0;i<10;i++) {
        Value row;row.kind='O';Value number;number.kind='I';number.integer=i;Value array;array.kind=i%2?'A':'L';
        for(int j=0;j<2*(1+i%3);j++)array.items.push_back(string_value(j%2?"bb":"a"));
        row.fields={{"num",number},{"arr",array}};nested.items.push_back(row);
    }
    auto options=EncodeOptions::optimized();
    auto e=encode_auto(nested,options);check(e.document.text()=="@32:AO2K3IAS");check(to_json(e.document.decode(e.data))==to_json(nested));
    std::cout<<"C++ optimization tests passed\n";
}
