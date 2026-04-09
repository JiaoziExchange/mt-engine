# SBE (Simple Binary Encoding) 集成指南

[English](SBE_INTEGRATION_GUIDE.md) | [中文](SBE_INTEGRATION_GUIDE_ZH.md)

## 适用于 MT-Engine 项目

---

## 1. 概述

### 1.1 什么是 SBE

Simple Binary Encoding (SBE) 是一个高性能二进制编码标准，最初由 FIX Protocol Limited 的高性能工作组于 2013 年开发，专门针对低延迟金融交易场景优化。

**SBE 的核心价值：**

| 特性 | 描述 |
|------|------|
| **零拷贝解码** | 直接从字节缓冲区读取，无需额外的内存分配 |
| **固定内存布局** | 消息结构固定大小，与二进制格式一一对应 |
| **缓存友好** | 字段顺序读取，CPU 缓存命中率极高 |
| **多语言支持** | Java、C++、C#、Go、Rust 等主流语言 |
| **流式处理** | 支持大消息流式编解码，无需全部加载内存 |

### 1.2 MT-Engine 的 SBE 集成目标

根据 MT-Engine 架构设计文档，所有对外暴露的 struct 都设计为固定大小，便于 SBE 生成的解析器处理。集成 SBE 的目标：

1. **命令序列化**：将 `Command` 结构序列化为二进制格式，用于网络传输或持久化
2. **结果序列化**：将 `CommandOutcome` / `Trade` 序列化为二进制格式
3. **跨语言互操作**：支持与其他语言（如 Java、C++）的系统进行消息交换
4. **高性能解析**：使用 SBE 生成的零拷贝解析器处理入站消息

---

## 2. SBE XML Schema 格式规范

### 2.1 基础结构

SBE 使用 XML 定义消息模式。根元素是 `<messageSchema>`：

```xml
<?xml version="1.0" encoding="UTF-8"?>
<messageSchema package="mt_engine"
               id="1000"
               semanticVersion="1.0.0"
               description="MT-Engine Binary Protocol"
               byteOrder="littleEndian">

    <!-- 类型定义区域 -->
    <types>
        <!-- ... -->
    </types>

    <!-- 消息定义区域 -->
    <messages>
        <!-- ... -->
    </messages>

</messageSchema>
```

**注意**：SBE XML Schema 中的根元素是 `<messageSchema>`，**不需要命名空间前缀** `sbe:`。

**关键属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `package` | 是 | 包名，用于 Java/C++/Rust 等语言的命名空间 |
| `id` | 是 | Schema 唯一标识符 |
| `semanticVersion` | 否 | 语义版本（如 "1.0.0"） |
| `version` | 否 | 版本号，默认 0 |
| `description` | 否 | Schema 描述 |
| `byteOrder` | 否 | `bigEndian` 或 `littleEndian`，默认 `littleEndian` |

**官方示例：**

```xml
<messageSchema package="uk.co.real_logic.sbe.examples"
               id="777"
               semanticVersion="5.2"
               description="Code generation unit test support"
               byteOrder="littleEndian">
```

### 2.2 基础类型 (Primitive Types)

SBE 支持以下原始类型：

| 类型 | 大小 | 说明 |
|------|------|------|
| `char` | 8 bit | 有符号字符 |
| `int8` | 8 bit | 有符号整数 |
| `int16` | 16 bit | 有符号整数 |
| `int32` | 32 bit | 有符号整数 |
| `int64` | 64 bit | 有符号整数 |
| `uint8` | 8 bit | 无符号整数 |
| `uint16` | 16 bit | 无符号整数 |
| `uint32` | 32 bit | 无符号整数 |
| `uint64` | 64 bit | 无符号整数 |
| `float` | 32 bit | IEEE 754 浮点 |
| `double` | 64 bit | IEEE 754 浮点 |

### 2.3 类型定义 (Types)

#### 2.3.1 基本类型别名

```xml
<!-- 基础类型别名：指定名称和原始类型 -->
<type name="Price" primitiveType="uint64" semanticType="Price"/>
<type name="Quantity" primitiveType="uint64" semanticType="Quantity"/>
<type name="OrderId" primitiveType="uint64" semanticType="OrderId"/>
<type name="Timestamp" primitiveType="uint64" semanticType="Timestamp"/>
```

**type 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 类型名称 |
| `primitiveType` | 是 | 原始数据类型 |
| `presence` | 否 | `required`、`optional`、`constant`，默认 `required` |
| `semanticType` | 否 | 语义类型描述 |
| `description` | 否 | 类型描述 |
| `length` | 否 | 数组长度（默认 1） |
| `characterEncoding` | 否 | 字符编码（如 UTF-8） |

#### 2.3.2 定长字节数组

```xml
<!-- 6 字节的车辆代码 -->
<type name="VehicleCode" primitiveType="char" length="6" semanticType="VehicleCode"/>

<!-- 常量字段示例 -->
<type name="maxRpm" primitiveType="uint16" presence="constant">9000</type>
<type name="fuel" primitiveType="char" presence="constant">Petrol</type>
```

### 2.4 枚举 (Enum)

```xml
<!-- 订单方向枚举 -->
<enum name="Side" encodingType="uint8" semanticType="Side">
    <validValue name="Buy">1</validValue>
    <validValue name="Sell">2</validValue>
</enum>

<!-- 时间效力枚举 -->
<enum name="TimeInForce" encodingType="uint8" semanticType="TimeInForce">
    <validValue name="GTC">0</validValue>   <!-- Good Till Cancel -->
    <validValue name="IOC">1</validValue>   <!-- Immediate Or Cancel -->
    <validValue name="FOK">2</validValue>   <!-- Fill Or Kill -->
    <validValue name="GTD">3</validValue>   <!-- Good Till Date -->
</enum>

<!-- 订单类型枚举 -->
<enum name="OrderType" encodingType="uint8" semanticType="OrderType">
    <validValue name="LIMIT">1</validValue>
    <validValue name="MARKET">2</validValue>
</enum>
```

**enum 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 枚举名称 |
| `encodingType` | 是 | 编码类型，必须是 `char`、`uint8` 或使用这些原始类型的命名类型 |
| `presence` | 否 | `required`、`optional`、`constant`，默认 `required` |
| `semanticType` | 否 | 语义类型描述 |
| `description` | 否 | 枚举描述 |

**validValue 子元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 枚举值名称 |
| `description` | 否 | 值描述 |

**Rust 生成结果示例：**

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Side {
    #[default]
    NullVal = 0,
    Buy = 1,
    Sell = 2,
}

impl From<u8> for Side {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::NullVal,
            1 => Self::Buy,
            2 => Self::Sell,
            _ => Self::NullVal,
        }
    }
}

impl From<Side> for u8 {
    fn from(v: Side) -> Self {
        match v {
            Side::Buy => 1,
            Side::Sell => 2,
            Side::NullVal => 0,
        }
    }
}
```

### 2.5 位集合 (BitSet)

```xml
<!-- 订单标志位 -->
<set name="OrderFlags" encodingType="uint16" semanticType="Flags">
    <choice name="POST_ONLY">0</choice>      <!-- 仅 Maker -->
    <choice name="REDUCE_ONLY">1</choice>   <!-- 只减仓 -->
    <choice name="ICEBERG">2</choice>       <!-- 冰山订单 -->
    <choice name="HIDDEN">3</choice>         <!-- 隐藏单 -->
</set>
```

**set 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 位集合名称 |
| `encodingType` | 是 | 编码类型，必须是 `uint8`、`uint16`、`uint32` 或 `uint64` |
| `presence` | 否 | `required`、`optional`、`constant`，默认 `required` |
| `semanticType` | 否 | 语义类型描述 |
| `description` | 否 | 位集合描述 |

**注意**：`choice` 元素的内容是**位位置**（bit position），而非位掩码。0 表示第 0 位，1 表示第 1 位，以此类推。

**Rust 生成结果示例：**

```rust
#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderFlags(pub u16);

impl OrderFlags {
    #[inline]
    pub fn get_post_only(&self) -> bool {
        0 != self.0 & (1 << 0)
    }

    #[inline]
    pub fn set_post_only(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 0)
        } else {
            self.0 & !(1 << 0)
        };
        self
    }

    // ... 其他标志位方法
}
```

### 2.6 复合类型 (Composite)

复合类型用于定义可重用的结构体，通常用于消息头和嵌套结构：

```xml
<!-- 消息头复合类型 -->
<composite name="MessageHeader" description="Message identifiers and length">
    <type name="blockLength" primitiveType="uint16"/>
    <type name="templateId" primitiveType="uint16"/>
    <type name="schemaId" primitiveType="uint16"/>
    <type name="version" primitiveType="uint16"/>
</composite>

<!-- 变长数据编码 -->
<composite name="VarDataEncoding">
    <type name="length" primitiveType="uint16" semanticType="Length"/>
    <type name="varData" primitiveType="uint16" length="0" characterEncoding="UTF-8"/>
</composite>

<!-- 重复组维度编码（官方默认命名） -->
<composite name="groupSizeEncoding" description="Repeating group dimensions">
    <type name="blockLength" primitiveType="uint16"/>
    <type name="numInGroup" primitiveType="uint8" semanticType="NumInGroup"/>
</composite>
```

**composite 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 复合类型名称 |
| `semanticType` | 否 | 语义类型描述 |
| `description` | 否 | 复合类型描述 |

**composite 内部元素：**

- `<type>`: 内联类型定义
- `<enum>`: 内联枚举定义
- `<set>`: 内联位集合定义
- `<ref>`: 引用其他已定义的类型

**ref 元素**：用于引用已定义的类型以实现复用：

```xml
<composite name="Booster">
    <enum name="BoostType" encodingType="char">
        <validValue name="TURBO">T</validValue>
        <validValue name="SUPERCHARGER">S</validValue>
    </enum>
    <type name="horsePower" primitiveType="uint8"/>
</composite>

<composite name="Engine" semanticType="Engine">
    <type name="capacity" primitiveType="uint16"/>
    <ref name="booster" type="Booster"/>
</composite>
```

### 2.7 消息定义 (Message)

**重要**：SBE XML Schema 中的消息元素是 `<message>`，**不需要命名空间前缀** `sbe:`。

消息结构中的元素顺序很重要：
1. 所有 `<field>` 必须放在 `<group>` 和 `<data>` 之前
2. 所有 `<group>` 必须在 `<data>` 之前
3. 固定长度字段在消息前部，重复组在中间，变长数据在最后

#### 2.7.1 简单消息示例

```xml
<!-- 注意：使用 <message> 而非 <sbe:message> -->
<message name="NewOrderSingle" id="1" description="订单提交命令" blockLength="48">
    <field name="cl_ord_id" id="11" type="ClOrdId"/>
    <field name="symbol" id="55" type="Symbol"/>
    <field name="side" id="54" type="Side"/>
    <field name="price" id="44" type="int64"/>
    <field name="quantity" id="38" type="uint64"/>
</message>
```

**注意**：官方 SBE 规范中 `<field>` 元素**不需要 `offset` 属性**，字段偏移量由代码生成器根据字段声明顺序自动计算。如果需要手动指定偏移量（如 IronSBE 特定实现），请参考相应工具的文档。

**message 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 消息名称 |
| `id` | 是 | 消息 ID（模板 ID） |
| `blockLength` | 是 | 固定字段块的长度（字节） |
| `description` | 否 | 消息描述 |
| `semanticType` | 否 | 语义类型 |

**field 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 字段名称 |
| `id` | 是 | 字段 ID（FIX 协议中的 Tag） |
| `type` | 是 | 字段类型（原始类型或已定义的类型） |
| `description` | 否 | 字段描述 |
| `semanticType` | 否 | 语义类型 |
| `offset` | 否 | 字段偏移量（特定实现可能需要） |
| `sinceVersion` | 否 | 字段添加的版本号，默认 0 |
| `presence` | 否 | `required`、`optional`、`constant` |
| `valueRef` | 否 | 引用枚举常量值，如 `valueRef="Model.C"` |

**官方 field 示例：**

```xml
<field name="serialNumber" id="1" type="uint32" semanticType="SerialNumber"/>
<field name="modelYear" id="2" type="ModelYear"/>
<field name="available" id="3" type="BooleanType"/>
<field name="code" id="4" type="Model"/>
<field name="discountedModel" id="8" type="Model" presence="constant" valueRef="Model.C"/>
```

#### 2.7.2 带重复组的消息

```xml
<!-- 重复组示例 -->
<group name="orders" id="10" dimensionType="groupSizeEncoding" blockLength="48">
    <field name="order_id" id="11" type="OrderId"/>
    <field name="side" id="12" type="Side"/>
    <field name="price" id="13" type="Price"/>
    <field name="quantity" id="14" type="Quantity"/>
</group>
```

**group 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 组名称 |
| `id` | 是 | 组 ID |
| `dimensionType` | 否 | 维度类型，默认 `groupSizeEncoding` |
| `blockLength` | 是 | 组内固定字段的长度（字节） |
| `description` | 否 | 组描述 |

**嵌套重复组示例：**

```xml
<group name="performanceFigures" id="12" dimensionType="groupSizeEncoding">
    <field name="octaneRating" id="13" type="uint8" semanticType="RON"/>
    <group name="acceleration" id="14" dimensionType="groupSizeEncoding">
        <field name="mph" id="15" type="uint16" semanticType="int"/>
        <field name="seconds" id="16" type="float" semanticType="int"/>
    </group>
</group>
```

#### 2.7.3 带变长数据的消息

```xml
<!-- 变长数据：订单标签 -->
<data name="tags" id="20" type="VarDataEncoding" semanticType="Tags"/>
```

**data 元素属性：**

| 属性 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 变长数据字段名称 |
| `id` | 是 | 字段 ID |
| `type` | 是 | 复合类型（必须包含 length 和 varData 部分） |
| `semanticType` | 否 | 语义类型 |
| `description` | 否 | 字段描述 |
| `sinceVersion` | 否 | 字段添加的版本号，默认 0 |

**官方变长数据编码示例：**

```xml
<composite name="varDataEncoding">
    <type name="length" primitiveType="uint16" semanticType="Length"/>
    <type name="varData" primitiveType="uint16" length="0" characterEncoding="UTF-8"/>
</composite>

<data name="make" id="17" type="varDataEncoding" semanticType="Make"/>
<data name="model" id="18" type="varDataEncoding" semanticType="Model"/>
```

#### 2.7.4 完整示例：MT-Engine 消息 Schema

```xml
<?xml version="1.0" encoding="UTF-8"?>
<messageSchema package="mt_engine"
               id="1000"
               semanticVersion="1.0.0"
               description="MT-Engine Binary Protocol for Order Matching"
               byteOrder="littleEndian">

    <!-- ==================== 类型定义 ==================== -->
    <types>
        <!-- 基础类型别名 -->
        <type name="Price" primitiveType="uint64" semanticType="Price"/>
        <type name="Quantity" primitiveType="uint64" semanticType="Quantity"/>
        <type name="OrderId" primitiveType="uint64" semanticType="OrderId"/>
        <type name="SequenceNumber" primitiveType="uint64" semanticType="SequenceNumber"/>
        <type name="Timestamp" primitiveType="uint64" semanticType="Timestamp"/>
        <type name="UserId" primitiveType="uint64" semanticType="UserId"/>
        <type name="Symbol" primitiveType="char" length="8"/>
        <type name="ClOrdId" primitiveType="char" length="20"/>

        <!-- 复合类型：消息头 -->
        <composite name="MessageHeader" description="Message identifiers">
            <type name="blockLength" primitiveType="uint16"/>
            <type name="templateId" primitiveType="uint16"/>
            <type name="schemaId" primitiveType="uint16"/>
            <type name="version" primitiveType="uint16"/>
        </composite>

        <!-- 复合类型：变长数据编码 -->
        <composite name="varDataEncoding" description="Variable length data encoding">
            <type name="length" primitiveType="uint16" semanticType="Length"/>
            <type name="varData" primitiveType="uint16" length="0" characterEncoding="UTF-8"/>
        </composite>

        <!-- 复合类型：重复组维度（官方默认命名） -->
        <composite name="groupSizeEncoding" description="Repeating group dimensions">
            <type name="blockLength" primitiveType="uint16"/>
            <type name="numInGroup" primitiveType="uint8" semanticType="NumInGroup"/>
        </composite>

        <!-- 枚举类型 -->
        <enum name="Side" encodingType="uint8" semanticType="Side">
            <validValue name="Buy">1</validValue>
            <validValue name="Sell">2</validValue>
        </enum>

        <enum name="OrderType" encodingType="uint8" semanticType="OrderType">
            <validValue name="LIMIT">1</validValue>
            <validValue name="MARKET">2</validValue>
        </enum>

        <enum name="TimeInForce" encodingType="uint8" semanticType="TimeInForce">
            <validValue name="GTC">0</validValue>
            <validValue name="IOC">1</validValue>
            <validValue name="FOK">2</validValue>
        </enum>

        <enum name="OrderStatus" encodingType="uint8" semanticType="OrderStatus">
            <validValue name="PENDING">0</validValue>
            <validValue name="FILLED">1</validValue>
            <validValue name="PARTIALLY_FILLED">2</validValue>
            <validValue name="CANCELLED">3</validValue>
            <validValue name="REJECTED">4</validValue>
        </enum>

        <enum name="CancelReason" encodingType="uint8" semanticType="CancelReason">
            <validValue name="USER_REQUESTED">0</validValue>
            <validValue name="POST_ONLY_WOULD_TAKE">1</validValue>
            <validValue name="IOC_NOT_FILLED">2</validValue>
            <validValue name="EXPIRED">3</validValue>
            <validValue name="SELF_TRADE">4</validValue>
        </enum>

        <!-- 位集合 -->
        <set name="OrderFlags" encodingType="uint16" semanticType="Flags">
            <choice name="POST_ONLY">0</choice>
            <choice name="REDUCE_ONLY">1</choice>
            <choice name="ICEBERG">2</choice>
        </set>
    </types>

    <!-- ==================== 消息定义 ==================== -->
    <messages>
        <!-- -------- 命令消息 -------- -->

        <!-- 订单提交命令 (48 bytes) -->
        <message name="NewOrderSingle" id="1" description="Submit a new order" blockLength="48">
            <field name="cl_ord_id" id="11" type="ClOrdId"/>
            <field name="symbol" id="55" type="Symbol"/>
            <field name="side" id="54" type="Side"/>
            <field name="price" id="44" type="int64"/>
            <field name="quantity" id="38" type="uint64"/>
        </message>

        <!-- 订单取消命令 -->
        <message name="OrderCancel" id="2" description="Cancel an existing order" blockLength="24">
            <field name="order_id" id="1" type="OrderId"/>
            <field name="timestamp" id="2" type="Timestamp"/>
            <field name="sequence_number" id="3" type="SequenceNumber"/>
        </message>

        <!-- 订单修改命令 -->
        <message name="OrderAmend" id="3" description="Amend an existing order" blockLength="32">
            <field name="order_id" id="1" type="OrderId"/>
            <field name="new_price" id="2" type="Price"/>
            <field name="new_quantity" id="3" type="Quantity"/>
            <field name="timestamp" id="4" type="Timestamp"/>
        </message>

        <!-- -------- 响应消息 -------- -->

        <!-- 成交记录 -->
        <message name="Trade" id="101" description="Trade execution report" blockLength="48">
            <field name="buy_order_id" id="1" type="OrderId"/>
            <field name="sell_order_id" id="2" type="OrderId"/>
            <field name="price" id="3" type="Price"/>
            <field name="quantity" id="4" type="Quantity"/>
            <field name="buy_user_id" id="5" type="UserId"/>
            <field name="sell_user_id" id="6" type="UserId"/>
        </message>

        <!-- 订单执行报告 -->
        <message name="OrderReport" id="102" description="Order execution report" blockLength="40">
            <field name="order_id" id="1" type="OrderId"/>
            <field name="status" id="2" type="OrderStatus"/>
            <field name="filled_qty" id="3" type="Quantity"/>
            <field name="remaining_qty" id="4" type="Quantity"/>
            <field name="avg_price" id="5" type="Price"/>
            <field name="cancel_reason" id="6" type="CancelReason"/>
            <field name="timestamp" id="7" type="Timestamp"/>
        </message>

        <!-- -------- 批量消息 -------- -->

        <!-- 批量订单提交 -->
        <message name="BatchSubmit" id="201" description="Batch order submission" blockLength="16">
            <field name="batch_id" id="1" type="uint64"/>
            <field name="timestamp" id="2" type="Timestamp"/>

            <!-- 重复组 -->
            <group name="orders" id="10" dimensionType="groupSizeEncoding" blockLength="48">
                <field name="cl_ord_id" id="11" type="ClOrdId"/>
                <field name="symbol" id="12" type="Symbol"/>
                <field name="side" id="13" type="Side"/>
                <field name="price" id="14" type="Price"/>
                <field name="quantity" id="15" type="Quantity"/>
            </group>
        </message>
    </messages>

</messageSchema>
```

---

## 3. Rust 代码生成

### 3.1 代码生成方案对比

SBE 项目主要有两种 Rust 代码生成方案：

| 方案 | 来源 | 特点 |
|------|------|------|
| **IronSBE** | joaquinbejar/IronSBE | 纯 Rust，完整功能集（含 Server/Client/Channel） |
| SBE 官方工具 | aeron-io/simple-binary-encoding | 需要 Java/Gradle，生成完整的编解码器 |
| sbe_gen | crates.io | 纯 Rust，轻量级，但功能不完整 |

**推荐方案**：对于 MT-Engine 项目，建议使用 **IronSBE**，因为：
- 纯 Rust 实现，无需安装 Java 环境
- 完整的功能集：Server、Client、SPSC/MPSC Channel
- 零拷贝编解码，业界领先性能（解码 1262M msg/sec）
- 100% Safe Rust，核心库无 unsafe 代码
- 完整的 SBE 功能支持（枚举、位集合、重复组、变长数据等）
- 支持 TCP/UDP/Multicast/SHM 多种传输方式

### 3.2 安装 IronSBE

```toml
# Cargo.toml
[dependencies]
ironsbe = "0.2"

[build-dependencies]
ironsbe-codegen = "0.2"
```

### 3.3 代码生成命令

#### 3.3.1 使用 build.rs

```rust
// build.rs
use std::fs;
use std::path::Path;

fn main() {
    // 告诉 Cargo 在 schema 文件变化时重新运行
    println!("cargo:rerun-if-changed=schemas/mt-engine/templates_FixBinary.xml");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let output_path = Path::new(&out_dir).join("mt_engine.rs");

    // 从 XML 文件生成 SBE 代码
    match ironsbe_codegen::generate_from_file(
        Path::new("schemas/mt-engine/templates_FixBinary.xml"),
    ) {
        Ok(code) => {
            // 将生成的代码写入 OUT_DIR
            fs::write(&output_path, code)
                .expect("Failed to write generated code");
            println!("cargo:rerun-if-changed={}", output_path.display());
        }
        Err(e) => {
            eprintln!("Failed to generate SBE code: {}", e);
            std::process::exit(1);
        }
    }
}
```

**注意**: 没有独立的 CLI 工具，代码生成通过 `ironsbe_codegen::generate_from_file` 或 `generate_from_xml` 函数在 build.rs 中完成。

### 3.4 构建 SBE 官方工具 (可选)

如果需要使用 SBE 官方工具生成更完整的代码：

```bash
# 克隆仓库
git clone https://github.com/aeron-io/simple-binary-encoding.git
cd simple-binary-encoding

# 使用 Gradle 构建
./gradlew :sbe-tool:jar

# 生成 Rust 代码
java -jar sbe-tool/build/libs/sbe-tool-*.jar \
    -Dsbe.target.language=Rust \
    -Dsbe.output.dir=./generated_rust \
    sbe-samples/src/main/resources/example-schema.xml
```

**Gradle 常用命令：**

| 命令 | 说明 |
|------|------|
| `./gradlew tasks` | 列出所有可用任务 |
| `./gradlew :sbe-tool:jar` | 构建 SBE 工具 |
| `./gradlew build` | 构建所有模块 |
| `./gradlew test` | 运行测试 |

### 3.5 生成代码结构

IronSBE 生成的代码结构如下：

```
src/generated/
└── mt_engine.rs      # 所有消息类型和编解码器
```

### 3.6 生成代码示例

IronSBE 生成的 `NewOrderSingle` 消息代码示例：

```rust
// src/generated/mt_engine.rs

use ironsbe::prelude::*;

// === 枚举类型 ===
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy = 1,
    Sell = 2,
}

// === 消息编码器 ===
pub struct NewOrderSingleEncoder<'a> {
    buffer: &'a mut [u8],
    offset: usize,
}

impl<'a> NewOrderSingleEncoder<'a> {
    pub fn wrap(buffer: &'a mut [u8], offset: usize) -> Self {
        Self { buffer, offset }
    }

    pub fn set_cl_ord_id(&mut self, v: &[u8; 20]) -> &mut Self {
        self.buffer[self.offset..self.offset + 20].copy_from_slice(v);
        self
    }

    pub fn set_symbol(&mut self, v: &[u8; 8]) -> &mut Self {
        self.buffer[self.offset + 20..self.offset + 28].copy_from_slice(v);
        self
    }

    pub fn set_side(&mut self, v: Side) -> &mut Self {
        self.buffer[self.offset + 28] = v as u8;
        self
    }

    pub fn set_price(&mut self, v: i64) -> &mut Self {
        self.buffer[self.offset + 29..self.offset + 37]
            .copy_from_slice(&v.to_le_bytes());
        self
    }

    pub fn set_quantity(&mut self, v: u64) -> &mut Self {
        self.buffer[self.offset + 37..self.offset + 45]
            .copy_from_slice(&v.to_le_bytes());
        self
    }

    pub fn encoded_length(&self) -> usize {
        48 // blockLength
    }
}

// === 消息解码器 ===
pub struct NewOrderSingleDecoder<'a> {
    buffer: &'a [u8],
    offset: usize,
}

impl<'a> NewOrderSingleDecoder<'a> {
    pub fn wrap(buffer: &'a [u8], offset: usize, _version: u16) -> Self {
        Self { buffer, offset }
    }

    pub fn cl_ord_id(&self) -> &[u8; 20] {
        unsafe {
            &*self.buffer[self.offset..self.offset + 20].as_ptr()
                .cast::<[u8; 20]>()
        }
    }

    pub fn symbol(&self) -> &[u8; 8] {
        unsafe {
            &*self.buffer[self.offset + 20..self.offset + 28].as_ptr()
                .cast::<[u8; 8]>()
        }
    }

    pub fn side(&self) -> Side {
        match self.buffer[self.offset + 28] {
            1 => Side::Buy,
            2 => Side::Sell,
            _ => Side::Buy,
        }
    }

    pub fn price(&self) -> i64 {
        i64::from_le_bytes(
            self.buffer[self.offset + 29..self.offset + 37].try_into().unwrap()
        )
    }

    pub fn quantity(&self) -> u64 {
        u64::from_le_bytes(
            self.buffer[self.offset + 37..self.offset + 45].try_into().unwrap()
        )
    }
}

pub const SCHEMA_VERSION: u16 = 1;
```

---

## 4. Rust API 使用说明

### 4.1 引入生成的代码

```rust
// src/lib.rs
mod mt_engine {
    include!(concat!(env!("OUT_DIR"), "/mt_engine.rs"));
}

pub use mt_engine::*;
```

### 4.2 消息编码

```rust
use ironsbe::prelude::*;
use mt_engine::{NewOrderSingleEncoder, Side};

fn encode_order_submit() {
    let mut buffer = [0u8; 256];

    // 创建编码器（自动写入消息头）
    let mut encoder = NewOrderSingleEncoder::wrap(&mut buffer, MessageHeader::ENCODED_LENGTH);

    // 设置字段（使用 snake_case 方法名）
    encoder
        .set_cl_ord_id(b"ORDER-001           ")
        .set_symbol(b"BTCUSDT ")
        .set_side(Side::Buy)
        .set_price(50000_0000)  // 50000.00 (8 位精度)
        .set_quantity(1_0000_0000);  // 1.00000000 BTC

    // 获取编码长度
    let len = encoder.encoded_length() + MessageHeader::ENCODED_LENGTH;

    // 发送 buffer[..len] 到网络
    println!("Encoded {} bytes", len);
}
```

### 4.3 消息解码（零拷贝）

```rust
use ironsbe::prelude::*;
use mt_engine::{NewOrderSingleDecoder, SCHEMA_VERSION};

fn decode_order_submit(buffer: &[u8]) {
    // 解析消息头
    let header = MessageHeader::decode(buffer, 0).unwrap();

    // 零拷贝解码（无需分配）
    let decoder = NewOrderSingleDecoder::wrap(
        &buffer[MessageHeader::ENCODED_LENGTH..],
        0,
        SCHEMA_VERSION,
    );

    // 直接从缓冲区访问字段
    println!("ClOrdId: {:?}", decoder.cl_ord_id());
    println!("Symbol: {:?}", decoder.symbol());
    println!("Side: {:?}", decoder.side());
    println!("Price: {}", decoder.price());
    println!("Quantity: {}", decoder.quantity());
}
```

### 4.4 使用消息头

```rust
use ironsbe_core::header::MessageHeader;
use mt_engine::{NewOrderSingleDecoder, SCHEMA_VERSION};

fn decode_with_header(buffer: &[u8]) {
    // 解析消息头
    let header = MessageHeader::decode(buffer, 0).unwrap();

    println!("Block Length: {}", header.block_length());
    println!("Template ID: {}", header.template_id());
    println!("Schema ID: {}", header.schema_id());
    println!("Version: {}", header.version());

    // 根据 template_id 选择正确的解码器
    match header.template_id() {
        1 => {
            let decoder = NewOrderSingleDecoder::wrap(
                &buffer[MessageHeader::ENCODED_LENGTH..],
                0,
                SCHEMA_VERSION,
            );
            println!("Price: {}", decoder.price());
        },
        _ => println!("Unknown template ID: {}", header.template_id()),
    }
}
```

### 4.5 处理重复组

```rust
use ironsbe::prelude::*;
use mt_engine::{MarketDataEncoder, MarketDataDecoder};

fn decode_market_data(buffer: &[u8]) {
    let decoder = MarketDataDecoder::wrap(buffer, 0, 1);

    println!("Seq: {}", decoder.seq());

    // 获取重复组（订单簿级别）
    for level in decoder.levels() {
        println!("Price: {}, Qty: {}", level.price(), level.qty());
    }
}

fn encode_market_data(buffer: &mut [u8]) {
    let mut encoder = MarketDataEncoder::wrap(buffer, 0);
    encoder.set_seq(12345);

    encoder.levels_mut().push(|level| {
        level.set_price(50000_0000);
        level.set_qty(10);
    });

    encoder.levels_mut().push(|level| {
        level.set_price(50001_0000);
        level.set_qty(5);
    });
}
```

### 4.6 枚举和位集合的互操作

```rust
use mt_engine::{Side, OrderFlags};

fn enum_conversion() {
    // 从原始值转换
    let side = Side::Buy;
    assert_eq!(side as u8, 1);

    // 匹配枚举
    match side {
        Side::Buy => println!("Buy side"),
        Side::Sell => println!("Sell side"),
    }
}

fn bitset_usage() {
    let mut flags = OrderFlags::default();

    // 设置标志位（通过原始字节操作）
    // flags.set_post_only(true);
}
```

---

## 5. MT-Engine 集成方案

### 5.1 项目结构建议

```
mt-engine/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── prelude.rs
│   ├── engine/
│   ├── book/
│   ├── types/
│   ├── command/
│   ├── outcome/
│   ├── sbe/                        # SBE 集成层
│   │   ├── mod.rs                  # 模块入口
│   │   ├── schema/                 # XML Schema 文件
│   │   │   └── mt-engine.xml
│   │   └── generated/              # 生成的代码
│   │       ├── mod.rs
│   │       └── ...                 # 消息模块
│   └── ...
├── schema/                          # 可选：独立的 schema 目录
│   └── mt-engine.xml
└── build.rs                        # 构建脚本（可选）
```

### 5.2 Cargo.toml 配置

```toml
[package]
name = "mt-engine"
version = "0.1.0"
edition = "2024"

[dependencies]
# 核心依赖
rustc-hash = "2.0"

# IronSBE 运行时依赖
ironsbe-core = "0.2"

[build-dependencies]
# IronSBE 代码生成
ironsbe-codegen = "0.2"

[features]
default = ["sparse"]
dense = []
sparse = []
serde = ["dep:serde"]
```

### 5.3 SBE 模块实现

```rust
// src/sbe/mod.rs

//! SBE (Simple Binary Encoding) 集成模块
//!
//! 本模块提供与外部系统的高性能二进制消息交换能力。
//! 使用 IronSBE 生成的零拷贝编解码器。

pub mod generated;

pub use generated::*;

use crate::{Command, CommandKind, Side, OrderId, Price, Quantity, Timestamp, SequenceNumber};
use ironsbe_core::header::MessageHeader;

/// 编码命令为二进制格式
pub fn encode_command(cmd: &Command, buf: &mut [u8]) -> Result<usize, EncodeError> {
    let offset = MessageHeader::ENCODED_LENGTH;

    match &cmd.kind {
        CommandKind::Submit(submit) => {
            let mut encoder = generated::NewOrderSingleEncoder::wrap(buf, offset);
            encoder.set_cl_ord_id(b"ORDER-001           ");  // 20 bytes
            encoder.set_symbol(b"BTCUSDT ");  // 8 bytes
            encoder.set_side(side_to_sbe(submit.order.side()));
            encoder.set_price(submit.order.price().0 as i64);
            encoder.set_quantity(submit.order.quantity().0);
            Ok(offset + encoder.encoded_length())
        }
        CommandKind::Cancel(cancel) => {
            let mut encoder = generated::OrderCancelEncoder::wrap(buf, offset);
            encoder.set_order_id(cancel.order_id.0);
            encoder.set_timestamp(cmd.meta.timestamp.0);
            encoder.set_sequence_number(cmd.meta.sequence_number.0);
            Ok(offset + encoder.encoded_length())
        }
        CommandKind::Amend(amend) => {
            // ...
        }
    }
}

/// 解码二进制数据为命令
pub fn decode_command(buf: &[u8]) -> Result<Command, DecodeError> {
    let header = MessageHeader::decode(buf, 0)?;
    let offset = MessageHeader::ENCODED_LENGTH;

    match header.template_id() {
        1 => {
            let decoder = generated::NewOrderSingleDecoder::wrap(&buf[offset..], 0, 1);
            Ok(Command::submit(
                SequenceNumber(decoder.sequence_number()),
                Timestamp(decoder.timestamp()),
                Order::limit(
                    OrderId(decoder.order_id()),
                    Price(decoder.price() as u64),
                    Quantity(decoder.quantity()),
                    side_from_sbe(decoder.side()),
                ),
            ))
        }
        2 => {
            let decoder = generated::OrderCancelDecoder::wrap(&buf[offset..], 0, 1);
            Ok(Command::cancel(
                SequenceNumber(decoder.sequence_number()),
                Timestamp(decoder.timestamp()),
                OrderId(decoder.order_id()),
            ))
        }
        _ => Err(DecodeError::UnknownTemplate(header.template_id())),
    }
}

/// 将 MT-Engine Side 转换为 SBE Side
fn side_to_sbe(side: Side) -> generated::Side {
    match side {
        Side::Buy => generated::Side::Buy,
        Side::Sell => generated::Side::Sell,
    }
}

/// 将 SBE Side 转换为 MT-Engine Side
fn side_from_sbe(sbe_side: generated::Side) -> Side {
    match sbe_side {
        generated::Side::Buy => Side::Buy,
        generated::Side::Sell => Side::Sell,
        _ => Side::Buy,
    }
}
```

### 5.4 与 MT-Engine 核心集成

```rust
// src/engine/mod.rs

use crate::sbe::{encode_command, decode_command};

impl<B: OrderBookBackend> Engine<B> {
    /// 从二进制缓冲区执行命令
    pub fn execute_from_buffer(&mut self, buf: &[u8]) -> Result<CommandOutcome, DecodeError> {
        let cmd = decode_command(buf)?;
        self.execute(cmd)
    }

    /// 执行命令并将结果编码到缓冲区
    pub fn execute_and_encode(
        &mut self,
        cmd_buf: &[u8],
        result_buf: &mut [u8],
    ) -> Result<usize, EncodeError> {
        let cmd = decode_command(cmd_buf)?;
        let outcome = self.execute(cmd)?;
        encode_outcome(&outcome, result_buf)
    }
}
```

---

## 6. 性能优化建议

### 6.1 SBE Flyweight 模式 vs 普通 Struct 模式

SBE 生成的代码支持两种模式：

| 模式 | 说明 | 适用场景 |
|------|------|----------|
| **Flyweight** | 直接映射缓冲区，零拷贝 | 高性能、低延迟场景 |
| **DTO** | 复制数据到独立 struct | 需要独立拥有数据时 |

**Flyweight 模式优势：**

```
普通模式：
┌─────────────────────────────────────┐
│ 缓冲区  ──copy──►  Struct  ──使用──► 丢弃 │
│ (64 bytes)     (64 bytes)           │
└─────────────────────────────────────┘
问题：每次解析都分配内存，GC 压力大

Flyweight 模式：
┌─────────────────────────────────────┐
│ 缓冲区  ──引用──► Flyweight  ──使用──► │
│ (64 bytes)      (16 bytes ptr)      │
└─────────────────────────────────────┘
优点：无分配、无拷贝、直接内存访问
```

### 6.2 MT-Engine 的性能优化策略

根据 MT-Engine 架构文档中的设计原则，建议以下优化：

#### 6.2.1 固定大小消息设计

所有对外消息设计为 64 字节倍数（缓存行对齐）：

```xml
<!-- 订单消息：64 bytes = 缓存行大小 -->
<message name="NewOrderSingle" blockLength="64">
    <!-- 热数据（8 字段 = 64 bytes）放在前面 -->
    <field name="remaining_qty" id="1" type="uint64" semanticType="Quantity"/>   <!-- 0-7 -->
    <field name="filled_qty" id="2" type="uint64" semanticType="Quantity"/>       <!-- 8-15 -->
    <field name="price" id="3" type="uint64" semanticType="Price"/>               <!-- 16-23 -->
    <field name="side_type" id="4" type="uint16" semanticType="Side"/>            <!-- 24-25 -->
    <field name="padding" id="5" type="uint16"/>                                   <!-- 26-27 -->
    <field name="order_id" id="6" type="uint64" semanticType="OrderId"/>            <!-- 28-35 -->
    <field name="sequence" id="7" type="uint64" semanticType="SequenceNumber"/>    <!-- 36-43 -->
    <field name="timestamp" id="8" type="uint64" semanticType="Timestamp"/>        <!-- 44-51 -->
    <field name="user_id" id="9" type="uint64" semanticType="UserId"/>             <!-- 52-59 -->
    <field name="reserved" id="10" type="uint32"/>                                 <!-- 60-63 -->
</message>
```

#### 6.2.2 热数据优先原则

```
缓存行布局优化前：
┌────────────────────────────────────────┐
│ cold1 │ hot1 │ cold2 │ hot2 │ cold3    │  ← 跨多个缓存行
└────────────────────────────────────────┘
问题：访问 hot1 时加载了不需要的 cold 字段

缓存行布局优化后：
┌────────────────────────────────────────┐
│ hot1 │ hot2 │ hot3 │ hot4              │  ← 完整装入一个缓存行
├────────────────────────────────────────┤
│ cold1 │ cold2 │ cold3                 │
└────────────────────────────────────────┘
优势：热路径只需加载一个缓存行
```

#### 6.2.3 批量预取策略

处理批量订单时，使用预取优化：

```rust
fn process_batch_orders(orders: &[OrderSubmit], engine: &mut Engine) {
    // 预取下一个订单到 L1 缓存
    for i in 0..orders.len() {
        // 处理当前订单
        engine.execute_from_message(&orders[i]);

        // 预取下一个订单（if 存在）
        if i + 1 < orders.len() {
            prefetch!(&orders[i + 1]);
        }
    }
}
```

### 6.3 字节序选择

| 字节序 | 优势 | 适用场景 |
|--------|------|----------|
| **littleEndian** | x86/x64 原生，无需转换 | MT-Engine 内部处理 |
| **bigEndian** | 网络字节序标准 | 与外部系统交换 |

**MT-Engine 建议**：内部使用 `littleEndian`，对外接口可提供字节序转换层。

---

## 7. 版本演进与兼容性

### 7.1 Schema 版本管理

SBE 支持 `sinceVersion` 属性添加新字段而不破坏兼容性：

```xml
<message name="NewOrderSingle" id="1" blockLength="64">
    <!-- 版本 1 字段 -->
    <field name="order_id" id="1" type="OrderId" sinceVersion="1"/>
    <field name="side" id="2" type="Side" sinceVersion="1"/>
    <field name="price" id="3" type="Price" sinceVersion="1"/>
    <field name="quantity" id="4" type="Quantity" sinceVersion="1"/>

    <!-- 版本 2 新增字段 -->
    <field name="client_order_id" id="5" type="uint64" sinceVersion="2"/>
    <field name="strategy_id" id="6" type="uint32" sinceVersion="2"/>
</message>
```

### 7.2 解码器版本处理

```rust
fn decode_with_version_check(buf: &[u8]) {
    let msg = NewOrderSingle::parse_prefix(buf).unwrap();

    // 检查支持的版本
    if header.version >= 2 {
        // 可以访问新字段
        if let Some(client_order_id) = msg.client_order_id_opt() {
            // ...
        }
    }
}
```

### 7.3 消息 ID 分配策略

```
消息 ID 分配表：

范围          用途              示例
─────────────────────────────────────────────
0-999         保留给协议       -
1-100         命令消息         1=NewOrderSingle, 2=OrderCancel
101-200       响应消息         101=Trade, 102=OrderReport
201-300       批量消息         201=BatchSubmit
301-400       管理消息         301=Heartbeat, 302=Reset
1001+         用户扩展          -
```

---

## 8. 最佳实践

### 8.1 Schema 设计原则

1. **固定大小优先**：所有消息头和主要字段使用固定大小
2. **热数据在前**：频繁访问的字段放在消息前面
3. **避免过度抽象**：保持 Schema 简洁明了
4. **语义类型标注**：使用 `semanticType` 描述字段业务含义
5. **版本规划**：预先考虑未来扩展，避免破坏性变更

### 8.2 错误处理

```rust
use ironsbe_core::error::Error as IronSbeError;

#[derive(Debug)]
pub enum SbeError {
    BufferTooSmall { required: usize, available: usize },
    InvalidTemplateId(u16),
    VersionMismatch { expected: u16, actual: u16 },
    InvalidEnumValue { field: &'static str, value: u8 },
    Io(std::io::Error),
}

impl From<IronSbeError> for SbeError {
    fn from(e: IronSbeError) -> Self {
        SbeError::InvalidFormat(e.to_string())
    }
}
```

### 8.3 测试策略

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ironsbe_core::header::MessageHeader;

    #[test]
    fn test_order_submit_roundtrip() {
        let mut buf = [0u8; 256];
        let offset = MessageHeader::ENCODED_LENGTH;

        // 编码
        let mut encoder = NewOrderSingleEncoder::wrap(&mut buf, offset);
        encoder.set_cl_ord_id(b"ORDER-001           ");
        encoder.set_symbol(b"BTCUSDT ");
        encoder.set_side(Side::Buy);
        encoder.set_price(50000_0000);
        encoder.set_quantity(1_0000_0000);

        // 解码
        let decoder = NewOrderSingleDecoder::wrap(&buf[offset..], 0, 1);
        assert_eq!(decoder.side(), Side::Buy);
    }

    #[test]
    fn test_schema_validation() {
        let schema = std::fs::read_to_string("schema/mt-engine.xml").unwrap();
        let result = ironsbe_schema::parse(&schema);
        assert!(result.is_ok());
    }
}
```

---

## 9. 参考资料

### 9.1 官方资源

| 资源 | 链接 |
|------|------|
| SBE 官网 | https://www.fixtrading.org/standards/sbe/ |
| SBE GitHub | https://github.com/aeron-io/simple-binary-encoding |
| **SBE XML Primer** | https://github.com/aeron-io/simple-binary-encoding/wiki/FIX-SBE-XML-Primer |
| **示例 Schema** | https://github.com/aeron-io/simple-binary-encoding/blob/master/sbe-samples/src/main/resources/example-schema.xml |
| **IronSBE Crate** | https://crates.io/crates/ironsbe |
| **IronSBE Docs** | https://docs.rs/ironsbe |
| **IronSBE GitHub** | https://github.com/joaquinbejar/IronSBE |

### 9.2 相关技术

| 技术 | 说明 |
|------|------|
| zerocopy | Rust 零拷贝序列化库 |
| Agrona | 高性能 Java 库，包含 SBE 所需的缓冲区实现 |
| Aeron | 基于 SBE 的高性能消息传输中间件 |

### 9.3 术语表

| 英文 | 中文 | 说明 |
|------|------|------|
| Flyweight | 轻量级模式 | 直接映射内存的模式 |
| Block Length | 块长度 | 固定字段区域的大小 |
| Template ID | 模板 ID | 消息类型的唯一标识 |
| Schema Version | Schema 版本 | 用于向前兼容 |
| Semantic Type | 语义类型 | 字段的业务含义描述 |

---

## 10. 附录

### 10.1 完整 Schema 文件模板

参考本文档第 2.7.4 节的完整示例。

### 10.2 常用命令速查

```bash
# 安装 IronSBE（通过 Cargo.toml）

# 安装 ironsbe-codegen CLI
cargo install --git https://github.com/joaquinbejar/IronSBE.git ironsbe-codegen

# 生成代码
ironsbe-codegen schemas/mt-engine.xml -o src/generated/

# 构建 SBE 官方工具（可选）
cd simple-binary-encoding && ./gradlew :sbe-tool:jar

# 使用官方工具生成 Rust 代码
java -jar sbe-tool/build/libs/sbe-tool-*.jar \
    -Dsbe.target.language=Rust \
    -Dsbe.rust.crate.version=0.1.0 \
    -Dsbe.output.dir=./generated \
    schema.xml
```

### 10.3 与 MT-Engine OrderData 的映射关系

```
MT-Engine OrderData (64 bytes)
┌────────────────────────────────────────┬────────────────────────────────────────┐
│ remaining_qty (0-7)                    │ filled_qty (8-15)                     │
├────────────────────────────────────────┼────────────────────────────────────────┤
│ price (16-23)                          │ side_order_type (24-25) │ padding (26-27)│
├────────────────────────────────────────┼────────────────────────────────────────┤
│ order_id (28-35)                       │ sequence_number (36-43)               │
├────────────────────────────────────────┼────────────────────────────────────────┤
│ timestamp (44-51)                      │ user_id (52-59)                        │
├────────────────────────────────────────┼────────────────────────────────────────┤
│ reserved (60-63)                       │                                        │
└────────────────────────────────────────┴────────────────────────────────────────┘

SBE NewOrderSingle 建议布局（保持一致）：
<field name="remaining_qty" id="1" type="uint64" semanticType="Quantity"/>
<field name="filled_qty" id="2" type="uint64" semanticType="Quantity"/>
<field name="price" id="3" type="uint64" semanticType="Price"/>
<field name="side_type" id="4" type="uint16" semanticType="Side"/>
<field name="padding" id="5" type="uint16"/>
<field name="order_id" id="6" type="uint64" semanticType="OrderId"/>
<field name="sequence_number" id="7" type="uint64" semanticType="SequenceNumber"/>
<field name="timestamp" id="8" type="uint64" semanticType="Timestamp"/>
<field name="user_id" id="9" type="uint64" semanticType="UserId"/>
<field name="reserved" id="10" type="uint32"/>
```

---

## 11. 与官方规范一致性说明

本指南根据 [FIX SBE XML Primer](https://github.com/aeron-io/simple-binary-encoding/wiki/FIX-SBE-XML-Primer) 官方文档编写，确保与 SBE 标准规范一致。

### 关键规范要点

1. **根元素命名**：使用 `<messageSchema>` 而非 `<sbe:messageSchema>`
2. **消息元素命名**：使用 `<message>` 而非 `<sbe:message>`
3. **无命名空间前缀**：SBE XML Schema 不需要命名空间前缀
4. **字段偏移量**：官方规范不要求 `offset` 属性，偏移量由代码生成器自动计算
5. **groupSizeEncoding**：官方推荐的重复组维度编码命名
6. **varDataEncoding**：官方推荐的变长数据编码命名
7. **presence 属性**：type、enum、set 元素支持 `presence` 属性（required/optional/constant）
8. **valueRef 属性**：field 元素支持 `valueRef` 引用枚举常量值
9. **ref 元素**：composite 内部可使用 `<ref>` 引用已定义类型

### IronSBE 特定说明

IronSBE 作为 SBE 的 Rust 实现，在某些方面可能有特定扩展（如 `offset` 属性支持）。如使用 IronSBE，请参考其特定文档。

---

*文档版本：1.2.0*
*最后更新：2026-04-06*
*适用项目：MT-Engine*
