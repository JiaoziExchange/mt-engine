# MT-Engine Architecture Design Document

[English](ARCHITECTURE.md) | [中文](ARCHITECTURE_ZH.md)

## 1. Overview

### 1.1 Background & Goals

MT-Engine is a high-performance, deterministic order matching engine library implemented in Rust. Designed specifically for trading systems, it follows the core principles of the LMAX architecture, implementing the matching engine as a single-threaded, deterministic, in-memory state machine.

**Core Design Goals:**

- **Deterministic Execution**: The same inputs must produce the same outputs, supporting backtesting, replay, and auditing.
- **Single-Threaded Design**: Avoid lock contention and synchronization overhead to achieve predictable low latency.
- **In-Memory State**: All states are stored in memory; persistence and network communication are handled externally.
- **Command-Result Model**: Clear input/output contracts for easy integration and testing.

### 1.2 Core Terminology

| Term | Definition |
|------|------|
| **Order Book** | Data structure recording all unexecuted resting orders. |
| **Matching** | The process of pairing buy and sell orders based on Price-Time priority. |
| **Taker** | The aggressive party initiating the trade, consuming market liquidity. |
| **Maker** | The passive party waiting for execution, providing market liquidity. |
| **Price Level** | A group of all orders at the same price. |
| **Time Priority** | Within the same price level, orders submitted earlier execute first. |
| **SBE (Simple Binary Encoding)** | High-performance binary encoding standard used for message serialization. |
| **Deterministic** | Given the same initial state and sequence of commands, the same result is guaranteed. |

### 1.3 Project Phases

| Phase | Status | Description |
|------|------|------|
| **Phase 1: SBE Protocol Layer** | ✅ Done | Binary message encoding/decoding supporting OrderSubmit/Cancel/Amend/Trade. |
| **Phase 2: Core Matching Engine** | ✅ Done | Implemented LTP-triggered stop-loss/take-profit, iceberg requeuing, and E2E strategies. |
| **Phase 3: Optimization (Sparse)** | ✅ Done | Full-path zero-allocation, SBE unwrap_unchecked, hardware prefetching, cache-line ABI alignment. |
| **Phase 4: Dense Engine & Scalability** | ✅ Done | Bitset-based tree-less backend, O(1) matching for dense assets. |
| **Phase 5: Decoupling & Modularization** | ✅ Done | Decoupled SBE encoding via `OrderEventListener`, extracted `ConditionOrderManager`. |

### 1.4 Design Principles

1. **Immutability over Mutability**: Prefer immutable data structures to reduce side effects.
2. **Explicit over Implicit**: API design emphasizes clarity over brevity.
3. **Composition over Inheritance**: Use composition to build complex types.
4. **Errors as Values**: Use `Result` types for error handling instead of exceptions.
5. **Zero-Cost Abstractions**: Abstractions (Generics/Traits) should not introduce runtime overhead.
6. **SBE Compatibility**: Exposed structs are designed with fixed sizes to facilitate SBE parser processing.
7. **Single Responsibility**: Decouple business logic from I/O and serialization protocols.

---

## 2. Project Structure

### 2.1 Code Organization

```
mt-engine/
├── mt-engine-core/         # Core Engine
│   └── src/
│       ├── engine/
│       │   ├── mod.rs              # Main Engine State Machine
│       │   ├── events.rs           # OrderEventListener Trait Definition
│       │   ├── sbe_listener.rs     # SBE Binary Encoder Implementation
│       │   └── condition.rs        # ConditionOrderManager (SL/TP logic)
│       ├── book/                   # Order Book Backends (Sparse/Dense)
│       ├── outcome/                # Execution results & SBE decoding
│       └── types/                  # Core domain types (Price, Qty, etc.)
├── mt-engine-sbe/          # SBE Encoding/Decoding Layer
├── schemas/                 # SBE XML Schema Definitions
└── docs/                    # Documentation
```

---

## 3. Core Engine Architecture

### 3.1 Overall Architecture

```mermaid
flowchart TB
    subgraph External["External System (User Code)"]
        Cmd[Command]
    end

    subgraph Core["MT-Engine Core"]
        Engine[Engine < B, L >]
        Cond[ConditionOrderManager]
    end

    subgraph Listeners["Event Handlers"]
        Trait[OrderEventListener Trait]
        Sbe[SbeEncoderListener]
        Mock[MockListener / ()]
    end

    subgraph Book["Order Book Component"]
        Backend[OrderBookBackend Trait]
        Bids[Bids]
        Asks[Asks]
    end

    Cmd -->|execute| Engine
    Engine <-->|delegates| Cond
    Engine -->|events| Trait
    Trait <|-- Sbe
    Trait <|-- Mock
    Engine <-->|trait bound| Backend
    Backend --> Bids
    Backend --> Asks
```

### 3.2 Decoupled Event Handling (Pattern A)

To maintain sub-100ns latency while improving code quality, MT-Engine uses **Static Dispatch via Generics** to decouple the matching loop from the SBE serialization protocol.

- **`OrderEventListener`**: A trait defining callbacks for trades, order acceptance, and cancellations.
- **Generic `Engine<B, L>`**: The engine is generic over both the backend `B` and the listener `L`.
- **Zero Overhead**: The Rust compiler monomorphizes and inlines the listener methods, resulting in zero runtime cost compared to direct buffer writing.

### 3.3 Modularized State Management

- **`ConditionOrderManager`**: Extracted from the `Engine` God Object. It manages Stop-Loss and Take-Profit orders using intrusive double-linked lists and BTreeMaps for O(1) triggering.
- **Memory Safety**: Uses `Slab` pools for stable memory layout and cache-friendly prefetching.

### 3.4 Dual Engine Backend Abstraction

To cater to different asset liquidity characteristics, MT-Engine implements a dual-backend strategy:

| Feature | `DenseBackend` (HFT) 🚀 | `SparseBackend` (General) 🧩 |
|------|---------------|-----------------|
| **Price Lookup** | L3 Bitset | BTreeMap |
| **Order Mapping** | Array + Free List | Slab + HashMap |
| **Queue Structure**| Intrusive Doubly Linked List | VecDeque / Intrusive Triggers |
| **Memory Alloc** | Pre-allocated | Dynamic (Slab Pool) |
| **Latency Expectation** | < 30ns | < 100ns (Optimized) |
| **Best For** | Mainstream assets (BTC/ETH) | Altcoins/NFTs/Long-tail |

---

## 4. Hardware Prefetching & Cache Line Alignment

### 4.1 OrderData Cache Alignment

```rust
#[repr(C, align(128))]
pub struct OrderData {
    // ========== [HOT DATA: Line 0 (64 bytes)] ==========
    pub remaining_qty: Quantity, 
    pub filled_qty: Quantity,    
    pub price: Price,           
    pub side: Side,             
    // ... other hot fields ...

    // ========== [COLD DATA: Line 1 (64 bytes)] ==========
    pub order_id: OrderId,      
    pub user_id: UserId,        
    // ... other cold fields ...
}
```

### 4.2 Hardware Prefetching

MT-Engine uses explicit prefetching to hide memory latency during order book traversal:
```rust
_mm_prefetch(&self.order_pool[next_idx].data as *const _ as *const i8, _MM_HINT_T0);
```

---

## 5. Time Complexity

| Operation | SparseBackend | DenseBackend |
|------|--------|-------|
| Best Bid/Ask | O(log N) | **O(1)** |
| Insert Order | O(log N) | O(1) |
| Cancel Order | **O(1)** (Intrusive) | **O(1)** |
| Match Execution| O(M log N) | O(M) |

*Where N is price levels, M is matched orders.*

---

## 6. Hardening & Stability

### 6.1 Snapshot Hardening
- **Zero-Copy Serialization**: Utilizes `rkyv` for high-performance memory-mapped snapshots.
- **CoW Forking**: Uses `libc::fork` to perform non-blocking snapshots via Copy-on-Write.

### 6.2 Transactional Consistency
- **Rollback Mechanism**: `execute_amend` ensures state integrity if matching fails.
- **Trigger Integrity**: Guaranteed cascade triggers for stop-loss/take-profit orders.
