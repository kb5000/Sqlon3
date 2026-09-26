# Sqlon 3：Schema/Data 二进制序列化格式

- 状态：项目协议规范
- 版本：3
- 日期：2026-09-26

本文件是 [SPEC.md](SPEC.md) 的中文译本。英文版是规范正文；如两者有差异，以英文版为准。

## 摘要

Sqlon 3 使用一份可读的 Schema 和一段二进制 Data 表示结构化值。Schema 可复用于多段 Data。协议覆盖 JSON 的六类值，并增加原始字节类型 Bytes。本文件翻译 Sqlon 3 的语法、字节布局、有效性规则、JSON 映射及合包方式；英文版是单一事实源。Sqlon 3 不要求兼容 Sqlon 2。

## 1. 约定与术语

本文的 **MUST**、**MUST NOT**、**SHOULD**、**MAY** 等大写关键词按 [BCP 14](https://www.rfc-editor.org/info/rfc8174/) 解释。字节均为八位组；“长度”若无另述，均指字节数。“Schema”是完整的 ASCII 字节序列，包括头部和一棵值 Schema；“Data”是与之配对的完整字节序列。

文档由 Schema 和 Data 两段组成。传输协议或文件容器负责给出两段的边界；Sqlon 3 不规定外层文件格式。第 7 节提供一个只发送单段 Data 的固定合包形式。

## 2. 数据模型

Sqlon 3 有十种编码类型：Null、Bool、Int、Double、Decimal、String、Bytes、List、Object、Array。Null、Bool、String、List、Object 分别对应 JSON 的 null、boolean、string、array、object；Array 也对应 JSON array；三种数值类型均对应 JSON number。Bytes 是额外的原始字节类型，按第 6 节转换为 JSON string。

编码类型与 JSON 类型不是一一对应关系。同一 JSON 值 **MAY** 有不同的 Sqlon 3 表示。有效的同一 Schema/Data 配对 **MUST** 能生成确定的 JSON 文本，Object 成员顺序除外。

## 3. Schema 头部

### 3.1. 头部

Schema **MUST** 全部是 ASCII 字节，且 **MUST** 以下述头部开始：

```text
@3<w>[;name=value]*:<value-schema>
```

其中 `w` 只能是 `2`、`4`、`8`，表示 Data 中长度字段的字节宽度。无扩展时，头部 `@3<w>:` 恰好占 4 字节。例如 `@32:AN` 的值 Schema 是 `AN`。

版本标识是 `@3`。其他版本或宽度 **MUST** 被拒绝；解码器 **MUST NOT** 猜测替代含义。

扩展是可选的 `;name=value` 项，位于宽度标记与冒号之间。`name` **MUST** 匹配 `[a-z][a-z0-9_]*`，`value` **MUST** 匹配 `[A-Za-z0-9._-]+`；同一 `name` **MUST NOT** 重复。解码器遇到未知扩展 **MUST** 报错；不得自动忽略，因为它可能改变解码含义。本版本未定义任何扩展。扩展语义须由本规范的后续修订或通信双方事先定义。
### 3.2. 长度字段

Data 中的长度和元素个数 **MUST** 采用头部指定宽度的无符号小端序整数。2、4、8 字节模式的最大值依次为 65535、4294967295、18446744073709551615。编码器 **MUST NOT** 截断超出范围的值。Int 和 Double 始终是 8 字节，宽度选项不改变它们。Schema 中的十进制计数不受此宽度限制，但受实际资源限制。

### 3.3. 传输层处理

压缩属于 Sqlon 3 编码之外的传输协议或文件容器。外层可以压缩消息，但必须在 Sqlon 解码前还原原始的 Schema 和 Data 字节。本规范不定义压缩扩展或压缩算法。

## 4. 值 Schema 语法

下列记号是描述性语法，尖括号、方括号和省略号不是实际字节。所有整数记号 `u` 是无前导零的 ASCII 十进制非负整数；零只能写作 `0`。

| 类型 | 值 Schema | 后续子 Schema |
| --- | --- | --- |
| Null | `N` | 无 |
| Bool | `B` | 无 |
| Int | `I` | 无 |
| Double | `D` | 无 |
| Decimal | `M` | 无 |
| String | `S` 或 `S<u>` | 无 |
| Bytes | `X` 或 `X<u>` | 无 |
| List | `L<u>` | 恰好 `u` 棵，顺序固定 |
| Object | `O<u>` 或 `O<u>K<u>` | 恰好第一个 `u` 指定的棵数，顺序固定 |
| Array | `A` | 恰好一棵元素 Schema |

`S` 和 `X` 不跟数字时为变长；`S0` 和 `X0` 是定长空值。`K<u>` 是 Object 的定长键修饰符，只能紧跟该 Object 的字段个数，不是值类型码。Object 的两个 `u` 分别表示字段个数和每个键的 UTF-8 字节数。

值 Schema **MUST** 恰好包含一棵完整的树。解码器 **MUST** 拒绝无效类型码、缺失子 Schema、非法数字、尾随 Schema 字节及过深或过大的 Schema。

## 5. Data 编码

| 类型 | 编码 |
| --- | --- |
| Null | 0 字节 |
| Bool | ASCII `T` 或 `F`，1 字节 |
| Int | 有符号 64 位二进制补码，小端序，8 字节 |
| Double | IEEE 754 binary64 位模式，小端序，8 字节 |
| Decimal | 长度字段，后接该长度的 ASCII JSON number 文本 |
| String | 变长：长度字段后接 UTF-8 字节；定长：仅 UTF-8 字节 |
| Bytes | 变长：长度字段后接原始字节；定长：仅原始字节 |
| List | 按顺序串接各子值的 Data，本身无长度字段 |
| Object | 按顺序串接各字段的键及对应值 Data |
| Array | 元素个数字段，后接各元素按同一元素 Schema 编码的 Data |

Decimal 的文本 **MUST** 至少有 1 字节，并 **MUST** 完整匹配 [RFC 8259](https://www.rfc-editor.org/info/rfc8259/) 的 JSON number 语法：`-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`。Double 的 NaN 和正负无穷 **MUST** 被拒绝。String 及 Object 键 **MUST** 是合法 UTF-8，解码后 **MUST** 是 Unicode 标量序列。Bytes 没有 UTF-8 约束。

变长 Object 的每个字段先写键的长度字段，再写键的 UTF-8 字节，最后写值 Data。若 Schema 带 `K<u>`，每个键 **MUST** 恰好占 `u` 字节，Data 中不写键长度，也不填充或截断。`K0` 允许空键。`K<u>` 只作用于所在 Object，不影响嵌套 Object；`u` 是 Schema 中的数字，不受 Data 长度宽度限制。单个 Object 内解码后的键 **MUST** 唯一。键和值 Schema 按位置对应；编码器 **MUST** 以相同顺序生成 Schema 和 Data。

List 的元素个数及每个元素 Schema 固定在 Schema，可异构。Array 的元素个数在 Data，可变化，但所有元素 **MUST** 符合唯一的元素 Schema。空 Array 仍 **MUST** 带元素 Schema，例如 `AN`。当元素 Schema 是 `N` 时，各元素占 0 字节，个数字段仍可确定 Array 的长度。

解码器 **MUST** 恰好消费Data。截断、长度溢出、无效标记、重复键、无效 UTF-8、无效 Decimal、与 Schema 不符的值以及尾随字节 **MUST** 被拒绝。

## 6. JSON 映射

Sqlon 3 到 JSON 的转换 **MUST** 生成符合 [RFC 8259](https://www.rfc-editor.org/info/rfc8259/) 的 UTF-8 JSON 文本。映射规则如下：

1. Null、Bool、String、List、Object、Array 分别输出 JSON null、boolean、string、array、object、array。
2. Int 输出无前导零的十进制整数文本。
3. Decimal 原样输出保存的 JSON number 文本，不经 binary64 转换。
4. Double 输出其有限 binary64 值的**精确**十进制展开，不用指数；小数部分去除末尾的零，整数不写小数点，负零输出 `-0`。这可能产生较长的数字文本，但不同实现的输出一致。
5. Bytes 输出 [RFC 4648 第 4 节](https://www.rfc-editor.org/info/rfc4648/) 标准字母表的 Base64 JSON string，带所需的 `=` 填充，不换行，未使用位必须为零。此规则仅用于 JSON 转换；Sqlon Data 中仍存原始字节。
6. JSON 字符串和 Object 键中的双引号输出为 `\"`，反斜杠输出为 `\\`。U+0008、U+0009、U+000A、U+000C、U+000D 依次输出为 `\b`、`\t`、`\n`、`\f`、`\r`；其他 U+0000 至 U+001F 输出为小写十六进制 `\u00xx`。其余 Unicode 标量直接输出为 UTF-8。生成器 **MUST NOT** 加 BOM 或多余空白。

Object 成员顺序 **MAY** 不同。String 和 Bytes 即使输出相同的 JSON string，仍是不同的 Sqlon 3 类型。本规范不要求 JSON 到 Sqlon 3 的编码唯一。RFC 8259 的语法可容纳孤立 UTF-16 代理项转义，但这种内容的互操作行为不稳定；Sqlon 3 的 String 和 Object 键只接受 Unicode 标量序列。该 UTF-8 限制是 Sqlon 3 的线格式规则，并非声称 JSON string 必须以 UTF-8 保存在内存中。

## 7. 合包格式

若需要一同发送一份内层 Schema 和对应的内层 Data，发送方 **MAY** 使用固定外层 Schema `@38:L2SX`，并仅传输外层 Data。外层 List 的第一项 `S` 是内层 Schema 的完整 ASCII 字节；第二项 `X` 是内层 Data 的原始字节。两项均为变长，各自有一个 8 字节小端序长度字段。外层及内层 Data 均遵循第 5 节的普通编码规则。

接收方先用固定外层 Schema 解出两项，再用内层 Schema 验证和解码内层 Data。两层的边界和有效性 **MUST** 分别检查。这个固定外层 Schema 不是额外的类型码，也不要求 Sqlon 3 文档必须合包。

## 8. 互操作示例

本节十六进制字节之间的空格仅为排版，不属于 Data。

| Schema | Data（十六进制） | 解码结果 / JSON |
| --- | --- | --- |
| `@32:N` | 空 | `null` |
| `@32:AN` | `03 00` | `[null,null,null]` |
| `@32:L2NN` | 空 | `[null,null]` |
| `@32:X` | `03 00 00 FF 42` | Bytes `00 FF 42` / `"AP9C"` |
| `@32:X3` | `00 FF 42` | 同上 |
| `@32:O1K3I` | `61 62 63 01 00 00 00 00 00 00 00` | `{"abc":1}` |

长度字段的宽度只改变 Data 中的长度和个数；不会改变 Int 或 Double 的 8 字节表示。

## 9. 安全与资源限制

实现 **SHOULD** 限制 Schema 深度、容器元素数、字节长度、总 Data 大小，并在分配内存前检查长度算术。解码器 **MUST NOT** 将未知扩展当作可忽略提示。Sqlon 3 不提供加密、认证或完整性保护；需要这些性质时，由外层传输提供。

## 10. 参考资料

- [BCP 14 / RFC 8174：规范性关键词](https://www.rfc-editor.org/info/rfc8174/)
- [RFC 8259：JSON Data Interchange Format](https://www.rfc-editor.org/info/rfc8259/)
- [RFC 4648：Base64 编码](https://www.rfc-editor.org/info/rfc4648/)

## 附录 A. 可选编码策略（资料性）

编码器可以提供显式选项，根据值选择 Schema。这些策略不增加线上的类型、头部
标记或解码模式；发送的 Schema 完整决定解码。优化编码必须保持 JSON 值及 Bytes
值；List 和 Array 可以表达同一个 JSON 数组。需要稳定、可复用 Schema 的应用应
显式提供 Schema。

本仓库实现提供默认关闭的独立选项：定长 String、定长 Bytes、同构 Array、定长
对象 key，以及紧凑长度宽度。`optimized()` 参数包启用这五项。压缩由外层传输协议或文件容器处理。

- String 和 key 的定长单位是 UTF-8 字节，Bytes 的单位是原始字节。不得截断或填充。
- 同构 Array 要求所有元素递归兼容同一 Schema；只比较最外层类型标签不够。
  长度不同的字符串可共享 `S`，等长字符串可共享 `S<n>`。不兼容的元素保留异构 List。
- 局部选择比较 Schema 字节数与所有对应实例的 Data 字节数之和。
  定长字段省下的长度前缀必须超过额外 Schema 数字的开销；Array 计算包含元素数前缀。
  大小相同时保留基础布局。这不保证全局最小编码。
- 紧凑宽度依次尝试 2、4、8 字节。实际写入 Data 的每一个长度或个数都必须适合该
  宽度的无符号范围。定长字段没有长度前缀；Int 和 Double 始终占八字节。

自动编码在 Data 编码前进行分析，可能分配中间结构；优化目标是表示大小，而非
CPU 延迟。按给定 Schema 编码和解码 Data 仍可顺序完成。
