# SBE (Simple Binary Encoding) Integration Guide

[English](SBE_INTEGRATION_GUIDE.md) | [中文](SBE_INTEGRATION_GUIDE_ZH.md)

## Applicable to the MT-Engine Project

---

## 1. Overview

### 1.1 What is SBE

Simple Binary Encoding (SBE) is a high-performance binary encoding standard, initially developed by the FIX Protocol Limited High-Performance Working Group in 2013, optimized specifically for low-latency financial trading scenarios.

**Core Values of SBE:**

| Feature | Description |
|------|------|
| **Zero-Copy Decoding** | Reads directly from the byte buffer without additional memory allocation. |
| **Fixed Memory Layout** | Fixed-size message structures map 1:1 to the binary format. |
| **Cache Friendly** | Sequential field reading results in extremely high CPU cache hit rates. |
| **Multi-Language Support** | Mainstream languages like Java, C++, C#, Go, and Rust. |
| **Streaming Processing** | Supports streaming encoding/decoding of large messages without loading everything into memory. |

### 1.2 Integration Goals in MT-Engine

According to the MT-Engine architecture design document, all exposed structs are designed with fixed sizes to facilitate processing by SBE-generated parsers. The goals of integrating SBE are:

1. **Command Serialization**: Serialize the `Command` structure into a binary format for network transmission or persistence.
2. **Result Serialization**: Serialize `ExecutionReport` / `PublicTrade` / `DepthUpdate` into a binary format.
3. **Cross-Language Interoperability**: Support message exchange with systems in other languages (e.g., Java, C++).
4. **High-Performance Parsing**: Process inbound messages using SBE's zero-copy parsers.

---

## 2. SBE XML Schema Specification

### 2.1 Basic Structure

SBE uses XML to define message schemas. The root element is `<messageSchema>`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<messageSchema package="mt_engine"
               id="1000"
               semanticVersion="1.0.0"
               description="MT-Engine Binary Protocol"
               byteOrder="littleEndian">

    <!-- Types Area -->
    <types>
        <!-- ... -->
    </types>

    <!-- Messages Area -->
    <messages>
        <!-- ... -->
    </messages>

</messageSchema>
```

**Note**: The root element in SBE XML Schema is `<messageSchema>`, **no namespace prefix** `sbe:` is needed.

**Key Attributes:**

| Attribute | Required | Description |
|------|------|------|
| `package` | Yes | Package name, used for namespace in Java/C++/Rust. |
| `id` | Yes | Unique identifier for the Schema. |
| `semanticVersion` | No | Semantic version (e.g., "1.0.0"). |
| `byteOrder` | No | `bigEndian` or `littleEndian`, defaults to `littleEndian`. |

### 2.2 Complete Example: MT-Engine Message Schema

```xml
<?xml version="1.0" encoding="UTF-8"?>
<messageSchema package="mt_engine"
               id="1000"
               semanticVersion="1.0.0"
               description="MT-Engine Binary Protocol for Order Matching"
               byteOrder="littleEndian">

    <!-- ==================== Type Definitions ==================== -->
    <types>
        <!-- Primitive Type Aliases -->
        <type name="Price" primitiveType="uint64" semanticType="Price"/>
        <type name="Quantity" primitiveType="uint64" semanticType="Quantity"/>
        <type name="OrderId" primitiveType="uint64" semanticType="OrderId"/>
        <type name="SequenceNumber" primitiveType="uint64" semanticType="SequenceNumber"/>
        <type name="Timestamp" primitiveType="uint64" semanticType="Timestamp"/>
        <type name="UserId" primitiveType="uint64" semanticType="UserId"/>
        
        <!-- Enums -->
        <enum name="Side" encodingType="uint8" semanticType="Side">
            <validValue name="Buy">1</validValue>
            <validValue name="Sell">2</validValue>
        </enum>
    </types>

    <!-- ==================== Message Definitions ==================== -->
    <messages>
        <!-- Order Submit Command (48 bytes) -->
        <message name="NewOrderSingle" id="1" description="Submit a new order" blockLength="48">
            <field name="cl_ord_id" id="11" type="ClOrdId"/>
            <field name="symbol" id="55" type="Symbol"/>
            <field name="side" id="54" type="Side"/>
            <field name="price" id="44" type="int64"/>
            <field name="quantity" id="38" type="uint64"/>
        </message>
    </messages>

</messageSchema>
```

---

## 3. Rust Code Generation

### 3.1 IronSBE Recommended

For MT-Engine, it is recommended to use **IronSBE** because:
- It is pure Rust, requiring no Java environment.
- Provides zero-copy encoding/decoding.
- Fully supports SBE features (Enums, Bitsets, Repeating Groups).

### 3.2 Installation

```toml
# Cargo.toml
[dependencies]
ironsbe = "0.2"

[build-dependencies]
ironsbe-codegen = "0.2"
```

### 3.3 Generating via build.rs

```rust
// build.rs
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=schemas/mt-engine/templates_FixBinary.xml");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let output_path = Path::new(&out_dir).join("mt_engine.rs");

    match ironsbe_codegen::generate_from_file(
        Path::new("schemas/mt-engine/templates_FixBinary.xml"),
    ) {
        Ok(code) => {
            fs::write(&output_path, code).unwrap();
        }
        Err(e) => panic!("Failed to generate SBE code: {}", e),
    }
}
```

---

## 4. Using the Rust API

### 4.1 Encoding an Order

```rust
use ironsbe::prelude::*;
use mt_engine::{NewOrderSingleEncoder, Side};

fn encode_order_submit() {
    let mut buffer = [0u8; 256];

    let mut encoder = NewOrderSingleEncoder::wrap(&mut buffer, MessageHeader::ENCODED_LENGTH);
    encoder
        .set_cl_ord_id(b"ORDER-001           ")
        .set_symbol(b"BTCUSDT ")
        .set_side(Side::Buy)
        .set_price(50000_0000)
        .set_quantity(1_0000_0000);
}
```

### 4.2 Decoding an Order (Zero-Copy)

```rust
use ironsbe::prelude::*;
use mt_engine::{NewOrderSingleDecoder, SCHEMA_VERSION};

fn decode_order_submit(buffer: &[u8]) {
    let decoder = NewOrderSingleDecoder::wrap(
        &buffer[MessageHeader::ENCODED_LENGTH..],
        0,
        SCHEMA_VERSION,
    );

    println!("Price: {}", decoder.price());
    println!("Quantity: {}", decoder.quantity());
}
```