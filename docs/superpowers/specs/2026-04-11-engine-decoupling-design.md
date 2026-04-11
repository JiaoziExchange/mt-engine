# Design Spec: Engine Decoupling and Refactoring

## 1. Overview
The current `Engine` struct in `mt-engine-core/src/engine/mod.rs` acts as a God Object, handling core matching logic, condition order management, snapshotting, and direct SBE (Simple Binary Encoding) serialization. 

This design aims to decouple the `Engine` by extracting non-core responsibilities and abstracting the event generation via Static Dispatch via Generics (Pattern A). This will improve code readability, testability, and maintainability while preserving the existing zero-cost performance profile.

## 2. Core Components

### 2.1. Trait `OrderEventListener`
A new trait will be introduced to handle events generated during the matching process.
```rust
pub trait OrderEventListener {
    fn on_order_accepted(&mut self, order: &OrderData, ts: Timestamp, offset: &mut usize);
    fn on_trade(&mut self, maker: &OrderData, taker: &OrderData, trade_qty: Quantity, trade_price: Price, ts: Timestamp, seq: SequenceNumber, offset: &mut usize);
    // Add other relevant events (cancel, amend, etc.)
}
```

### 2.2. Generic `Engine`
The `Engine` will be generic over the `OrderEventListener`.
```rust
pub struct Engine<'a, B: OrderBookBackend = SparseBackend, L: OrderEventListener = SbeEncoderListener<'a>> {
    // ... existing fields ...
    listener: L,
}
```

### 2.3. SBE Encoder Listener
The SBE encoding logic currently tangled within `match_order` will be moved into an implementation of `OrderEventListener`.

### 2.4. Module Extraction
- **Condition Orders**: Move `stop_buy_triggers`, `condition_order_store`, etc., and their related logic (`register_condition_order`, `process_triggered_orders`) to a new `ConditionOrderManager` or similar structure, potentially within `src/engine/condition.rs`.
- **Snapshots**: Move snapshotting logic (`to_snapshot`, `trigger_snapshot_rkyv`) to a `SnapshotController` or keep it isolated in a `snapshot.rs` module if it heavily depends on Engine internals.

## 3. Execution Plan
1.  **Define `OrderEventListener`**: Create the trait and event structures.
2.  **Extract SBE logic**: Create `SbeEncoderListener` implementing the trait.
3.  **Refactor `Engine`**: Update `Engine` to take the generic listener and call the trait methods instead of direct SBE encoding.
4.  **Extract Condition Orders**: Move condition order logic to a separate struct/module.
5.  **Extract Snapshots**: Move snapshot logic to a separate struct/module.
6.  **Update Tests**: Ensure all existing tests pass with the new architecture. Add a `MockListener` for better unit testing of the matching logic.

## 4. Open Questions / Trade-offs
- The lifetime `'a` might need careful propagation if the `SbeEncoderListener` holds the mutable reference to the `response_buffer`.
- Moving Condition Orders out of Engine might require passing `ltp` and `book` back and forth. We need to define clear boundaries.
