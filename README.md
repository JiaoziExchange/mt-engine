# MT-Engine

[English](README.md) | [中文](README_ZH.md)

**MT-Engine** is a high-frequency trading matching engine core built with Rust. It achieves extreme performance and zero-allocation on the hot path. It is designed for ultra-low latency trading scenarios and natively supports SBE (Simple Binary Encoding).

---

## ⚡ Performance & Dual Engine Architecture

MT-Engine innovatively provides two order book backends, allowing the system to switch at runtime based on the liquidity characteristics of the trading pairs. Both achieve zero-allocation and fully exploit CPU cache efficiency:

### 1. Performance Benchmarks
Benchmarked under **50,000 order steady-state saturation** with strictly **monotonic OrderIDs**, our shootout covers the complete lifecycle (Submit + Match/Cancel) using a `MixedWorkload` (Standard, Iceberg, Stop, Post-Only):

| Backend Configuration | Snapshot Features | Avg Latency (ns/op) | Performance Notes |
| :--- | :---: | :---: | :--- |
| **`DenseBackend`** | OFF | **~15.6 ns** | 🚀 **O(1)** logic, zero-leak steady state |
| **`SparseBackend`** | OFF | ~30.8 ns | 🧩 Memory efficient (O(log N)) |
| **`SparseBackend`** | **ON** | **~30.8 ns** | 🛡️ **Zero-Cost Abstraction verified** |
| **`Dense + Snapshot`**| N/A | Mutually Exclusive | Use `serde` for direct array persistence |

*(Note: These benchmarks reflect high-fidelity steady-state performance. The inclusion of **O(1) OrderID Monotonicity Guards** has further optimized the hot path by removing redundant map lookups for uniqueness.)*

### 2. Architecture Differences

| Core Feature | `DenseBackend` 🚀 | `SparseBackend` 🧩 |
| :--- | :--- | :--- |
| **Best Use Case** | **Mainstream core assets (e.g., BTC/ETH)** requiring ultra-low latency | **Altcoins, long-tail assets**, many trading pairs, dispersed liquidity |
| **Underlying Data Structure** | `L3 Bitset` + Static Array + Intrusive Doubly Linked List | `BTreeMap` + `Slab` + `HashMap` |
| **Best Bid/Ask Search** | **O(1)** (Using hardware CTZ bitwise operations) | **O(log N)** (Red-Black Tree navigation) |
| **Order Queuing & Cancel** | **O(1) / O(1)** (Intrusive pointers, in-place removal) | **O(log N) / O(N)** (VecDeque based, requires traversal) |
| **Memory Allocation** | One-time pre-allocation at initialization, no runtime resizing overhead | Dynamic allocation at runtime (Slab and Map resize as needed) |
| **Footprint**| **Extremely High** (Depends on `PriceRange` and capacity) | **Extremely Low** (Allocated on demand, strictly proportional to active orders) |

---

## 🚀 Core Features

- 🛡️ **SBE Native**: Fast path processing of binary messages based on Simple Binary Encoding.
- 🩸 **Zero-Allocation**: The hot path (including conditional order cascading triggers) completely eliminates any dynamic memory allocation.
- 🧊 **Cache Optimized**: `OrderData` uses 128-byte alignment, combined with `_mm_prefetch` hardware prefetching and SoA abstraction to maximize CPU cache hit rates.
- 🔄 **Advanced Features**: Natively supports iceberg orders, stop-loss triggers (O(1) cascading trigger pool), Post-Only slippage control, GTD/IOC/FOK, and end-to-end validation.
- 🛡️ **ID Integrity Guard**: Built-in $O(1)$ validation to enforce strictly monotonic and unique OrderIDs, preventing state corruption at the gateway.

---

## 🛡️ Integration Requirements

To achieve ultra-low latency and maintain system integrity, integration with MT-Engine must respect the following constraints:

### 1. Strictly Monotonic OrderIDs
Order IDs MUST be **strictly increasing** and **unique** for each symbol/engine instance:
- **Requirement**: `new_id > last_order_id`. In **Dense Mode**, the ID must also fall within `[1, capacity]` for deterministic physical indexing (managed by OMS via ID recycling).
- **Validation**: Engine performs an $O(1)$ hardware-friendly numerical check at the entry point.
- **Reaction**: Requests with duplicate or regressing IDs are immediately rejected with `DuplicateOrderId`.
- **Reason**: This eliminates the need for expensive hash map lookups for uniqueness checks, maintaining sub-20ns latency.

### 2. SBE Protocol Standard
All commands and reports follow the **Simple Binary Encoding (SBE)** standard:
- **Zero-Allocation**: Messages are decoded and encoded directly in pre-allocated buffers.
- **Fixed-Width Offset**: Efficient field access without variable-length parsing overhead.
- **Schema**: Use the provided XML schemas in `/schemas` to generate your language-specific encoders.

---

---

## 📖 User Guide

### 1. Constructing & Executing Orders
Use `CommandCodec` for lock-free, zero-allocation message construction. The engine supports two order book backends:
- `SparseBackend`: Based on BTreeMap, suitable for long-tail assets with sparse prices.
- `DenseBackend`: Based on L3 Bitset and pre-allocated arrays, suitable for mainstream assets with dense prices, providing O(1) extreme performance.

```rust
use mt_engine_core::prelude::*;
use mt_engine_core::codec::CommandCodec;
use mt_engine_core::book::backend::dense::{DenseBackend, PriceRange};
use mt_engine_core::book::backend::sparse::SparseBackend;

// 1. Initialize engine and buffers
let mut resp_buf = [0u8; 1024];
let mut cmd_buf = [0u8; 1024];

// Option A: Use the general Sparse Engine
// let mut engine = Engine::new(SparseBackend::new(), &mut resp_buf);

// Option B: Use the high-performance Dense Engine (e.g., Price range 100~200, tick=1, capacity 1024)
let config = PriceRange { min: Price(100), max: Price(200), tick: Price(1) };
let mut engine = Engine::new(DenseBackend::new(config, 1024), &mut resp_buf);
let mut codec = CommandCodec::new(&mut cmd_buf);

// 2. Construct a Post-Only order
let mut flags = OrderFlags::new(0);
flags.set_post_only(true);

let cmd = codec.encode_submit_ext(
    0,                      // buffer offset
    OrderId(1001),          // order_id
    UserId(201),            // user_id
    Side::buy,
    OrderType::limit,
    Price(10000),           // price
    Quantity(100),          // quantity
    SequenceNumber(1),      // sequence
    Timestamp(1712460000),  // timestamp
    TimeInForce::gtc,
    flags
);

// 3. Execute matching
let outcome = engine.execute_submit(&cmd);
match outcome {
    CommandOutcome::Applied(report) => println!("Order Placed: {:?}", report.status),
    CommandOutcome::Rejected(fail) => println!("Rejected: {:?}", fail),
}
```

### 2. Running Performance Tests
```bash
# Run strategy-specific benchmarks
cargo bench -p mt-engine-core --bench matching_engine -- Strategies
```

---

## 🛠️ Feature Matrix

| Feature | Status | Details |
| :--- | :---: | :--- |
| **GTC / IOC / FOK** | ✅ | Fully supported |
| **GTD / GTH** | ✅ | Millisecond-level lazy expiration |
| **Post-Only** | ✅ | Price crossing interception (Lazy Validation) |
| **Iceberg** | ✅ | Auto-refresh, FIFO re-queuing, and hidden penetration |
| **Stop / Stop-Limit** | ✅ | LTP O(1) triggering with zero-allocation cascading pool |
| **Self-Trade Prevention** | 🏛️ | Recommended to handle at the OMS layer |

---

## Developer Documentation

- [📜 Transaction Types](docs/TRANSACTION_TYPES.md)
- [🏗️ Architecture](docs/ARCHITECTURE.md)
- [🔌 SBE Integration Guide](docs/SBE_INTEGRATION_GUIDE.md)

---

## 📜 License

This project is licensed under the **Apache License 2.0**. For details, please see the [LICENSE](LICENSE) file in the project root.