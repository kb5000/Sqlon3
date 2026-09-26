#pragma once
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace sqlon3 {
struct Schema {
    char kind = 'N';
    std::optional<std::size_t> size;
    std::vector<Schema> children;
    std::optional<std::size_t> key_bytes;
};
struct Value {
    char kind = 'N';
    bool boolean = false;
    std::int64_t integer = 0;
    double real = 0;
    std::string text;
    std::vector<std::uint8_t> bytes;
    std::vector<Value> items;
    std::vector<std::pair<std::string, Value>> fields;
};
struct Document {
    int width = 8;
    Schema root;
    static Document parse(const std::string& text);
    std::string text() const;
    std::vector<std::uint8_t> encode(const Value& value) const;
    Value decode(const std::vector<std::uint8_t>& data) const;
};
std::string to_json(const Value& value);
std::vector<std::uint8_t> pack(const Document& document, const std::vector<std::uint8_t>& data);
std::pair<Document, std::vector<std::uint8_t>> unpack(const std::vector<std::uint8_t>& data);
}

namespace sqlon3 {
struct EncodeOptions {
    bool fixed_strings=false, fixed_bytes=false, homogeneous_arrays=false;
    bool fixed_keys=false, compact_lengths=false;
    int width=8;
    static EncodeOptions optimized() {
        EncodeOptions o; o.fixed_strings=o.fixed_bytes=o.homogeneous_arrays=o.fixed_keys=o.compact_lengths=true;return o;
    }
};
struct Encoded { Document document; std::vector<std::uint8_t> data; };
Encoded encode_auto(const Value& value, const EncodeOptions& options = {});
}
