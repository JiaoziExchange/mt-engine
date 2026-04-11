# Engine Decoupling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the core matching engine to decouple business logic from SBE binary encoding using Static Dispatch via Generics (Pattern A).

**Architecture:** Introduce an `OrderEventListener` trait. Make `Engine` generic over this trait. Implement the trait in a new `SbeEncoderListener`. Extract condition order and snapshot logic to simplify the `Engine` struct.

**Tech Stack:** Rust, generic programming, traits.

---

### Task 1: Define `OrderEventListener` Trait and Event Types

**Files:**
- Create: `mt-engine-core/src/engine/events.rs`
- Modify: `mt-engine-core/src/engine/mod.rs`
- Modify: `mt-engine-core/src/lib.rs` (if necessary to export the module)

- [ ] **Step 1: Write the basic trait structure**

Create `mt-engine-core/src/engine/events.rs`:
```rust
use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};

pub trait OrderEventListener {
    /// Called when a trade occurs.
    fn on_trade(
        &mut self,
        maker: &OrderData,
        taker: &OrderData,
        trade_qty: Quantity,
        trade_price: Price,
        ts: Timestamp,
        seq: SequenceNumber,
        trade_id: u64,
        offset: &mut usize,
    );

    // Placeholder for other events (e.g., on_cancel, on_amend) if SBE needs them directly
    // For now, we focus on `on_trade` as it's the core SBE operation in the match loop.
}
```

- [ ] **Step 2: Add to module tree**

In `mt-engine-core/src/engine/mod.rs`:
```rust
pub mod events;
pub use events::OrderEventListener;
```

- [ ] **Step 3: Compile check**

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add mt-engine-core/src/engine/events.rs mt-engine-core/src/engine/mod.rs
git commit -m "refactor(engine): add OrderEventListener trait"
```

### Task 2: Implement `SbeEncoderListener`

**Files:**
- Create: `mt-engine-core/src/engine/sbe_listener.rs`
- Modify: `mt-engine-core/src/engine/mod.rs`

- [ ] **Step 1: Write the implementation**

Create `mt-engine-core/src/engine/sbe_listener.rs`:
```rust
use crate::engine::events::OrderEventListener;
use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};
use mt_engine::message_header_codec;
use mt_engine::trade_codec;
use mt_engine::WriteBuf;

pub struct SbeEncoderListener<'a> {
    pub response_buffer: &'a mut [u8],
}

impl<'a> SbeEncoderListener<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { response_buffer: buffer }
    }
}

impl<'a> OrderEventListener for SbeEncoderListener<'a> {
    #[inline(always)]
    fn on_trade(
        &mut self,
        maker: &OrderData,
        taker: &OrderData,
        trade_qty: Quantity,
        trade_price: Price,
        ts: Timestamp,
        seq: SequenceNumber,
        trade_id: u64,
        offset: &mut usize,
    ) {
        let trade_offset = *offset;
        let trade_buf = WriteBuf::new(&mut self.response_buffer[..]);
        let trade_encoder = trade_codec::encoder::TradeEncoder::default().wrap(
            trade_buf,
            trade_offset + message_header_codec::ENCODED_LENGTH,
        );
        let mut header_encoder = trade_encoder.header(trade_offset);

        header_encoder.block_length(trade_codec::SBE_BLOCK_LENGTH);
        header_encoder.template_id(trade_codec::SBE_TEMPLATE_ID);
        header_encoder.schema_id(mt_engine::SBE_SCHEMA_ID);
        header_encoder.version(mt_engine::SBE_SCHEMA_VERSION);

        let mut trade_encoder = unsafe { header_encoder.parent().unwrap_unchecked() };
        trade_encoder.trade_id(trade_id);
        trade_encoder.maker_order_id(maker.order_id.0);
        trade_encoder.taker_order_id(taker.order_id.0);
        trade_encoder.side(taker.side);
        trade_encoder.price(trade_price.0);
        trade_encoder.quantity(trade_qty.0);
        trade_encoder.timestamp(ts.0);
        trade_encoder.sequence_number(seq.0);

        *offset += message_header_codec::ENCODED_LENGTH + trade_codec::SBE_BLOCK_LENGTH as usize;
    }
}
```

- [ ] **Step 2: Add to module tree**

In `mt-engine-core/src/engine/mod.rs`:
```rust
pub mod sbe_listener;
pub use sbe_listener::SbeEncoderListener;
```

- [ ] **Step 3: Compile check**

Run: `cargo check`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add mt-engine-core/src/engine/sbe_listener.rs mt-engine-core/src/engine/mod.rs
git commit -m "refactor(engine): implement SbeEncoderListener"
```

### Task 3: Make `Engine` Generic over `OrderEventListener`

**Files:**
- Modify: `mt-engine-core/src/engine/mod.rs`

- [ ] **Step 1: Update `Engine` definition**

In `mt-engine-core/src/engine/mod.rs`, replace `response_buffer` with `listener`:

```rust
// Find:
// pub struct Engine<'a, B: OrderBookBackend = SparseBackend> {
// ...
//     /// 用户提供的响应缓冲区 (Zero-Allocation & External Management)
//     response_buffer: &'a mut [u8],

// Replace with:
pub struct Engine<B: OrderBookBackend = SparseBackend, L: OrderEventListener = ()> {
    pub backend: B,
    pub(crate) last_sequence_number: SequenceNumber,
    pub(crate) last_timestamp: Timestamp,
    pub(crate) trade_id_seq: u64,

    /// 统一事件监听器 (替代直接的 response_buffer)
    pub listener: L,
    
    // ... other fields remain exactly the same ...
```
*(Note: You will need to remove the `'a` lifetime from `Engine` and its `impl` block, or keep it if `SbeEncoderListener` forces it. Let's try to remove it from `Engine` itself and let the generic parameter `L` handle its own lifetimes if possible. However, the existing code has `impl<'a, B> Engine<'a, B>`. We'll change it to `impl<B, L> Engine<B, L> where B: OrderBookBackend, L: OrderEventListener`)*

- [ ] **Step 2: Update `Engine::new`**

```rust
// Change:
// impl<'a, B: OrderBookBackend> Engine<'a, B> {
//     pub fn new(backend: B, buffer: &'a mut [u8]) -> Self {

// To:
impl<B: OrderBookBackend, L: OrderEventListener> Engine<B, L> {
    pub fn new(backend: B, listener: L) -> Self {
        Self {
            backend,
            last_sequence_number: SequenceNumber(0),
            last_timestamp: Timestamp(0),
            trade_id_seq: 0,
            listener, // Replaced buffer
            // ... initialize other fields
```

- [ ] **Step 3: Update `match_order` to use listener**

Inside `match_order`:
```rust
// Replace the SBE encoding block:
// let trade_offset = *offset;
// let trade_buf = WriteBuf::new(self.response_buffer);
// ...
// *offset += message_header_codec::ENCODED_LENGTH + trade_codec::SBE_BLOCK_LENGTH as usize;

// With:
self.listener.on_trade(
    &maker_order.data,
    taker_order,
    Quantity(trade_qty),
    opp_price,
    ts,
    seq,
    self.trade_id_seq,
    offset,
);
```

- [ ] **Step 4: Update usages of `self.response_buffer` in payloads**

Currently, things like `payload: &self.response_buffer[..offset]` exist.
If `Engine` no longer holds `response_buffer`, it can't return it directly in `CommandReport`.
**Design Decision:** The listener should own the buffer. `CommandOutcome` might need to be adjusted, or the caller retrieves the buffer from the listener.
*Wait, `CommandOutcome::Applied(CommandReport { payload: &self.response_buffer[..offset] })` requires the slice. If `listener` holds it, we need a way to get it back.*

Add a method to `OrderEventListener`:
```rust
// In events.rs:
fn get_payload(&self, offset: usize) -> &[u8];
```
Implement in `SbeEncoderListener`:
```rust
fn get_payload(&self, offset: usize) -> &[u8] {
    &self.response_buffer[..offset]
}
```
Update `CommandReport` usages in `mod.rs`:
```rust
payload: self.listener.get_payload(*offset), // or offset for the specific scope
```
*(Note: Rust's borrow checker might complain about returning `&[u8]` from `&mut self.listener` while returning `CommandOutcome<'_>`. A simpler fix for the refactoring step is to let the user of `Engine` pass the buffer, but keep `listener` mutable. Actually, `CommandOutcome` is short-lived. Let's see if we can just define `fn get_payload<'a>(&'a self, offset: usize) -> &'a [u8]`)*

- [ ] **Step 5: Fix compilation errors**

Run: `cargo check` and fix any lifetime/borrowing issues arising from removing `response_buffer` from `Engine` and placing it in `L`.
*(If lifetimes become too complex in this step, it is acceptable to temporarily leave `response_buffer: &'a mut [u8]` in `Engine` JUST for returning payloads, and only pass `&mut response_buffer` to a stateless static method, but Pattern A prefers the listener holding state. Let's stick to Pattern A and fix borrow errors.)*

- [ ] **Step 6: Commit**

```bash
git add mt-engine-core/src/engine/mod.rs mt-engine-core/src/engine/events.rs mt-engine-core/src/engine/sbe_listener.rs
git commit -m "refactor(engine): inject OrderEventListener into Engine"
```

### Task 4: Extract Condition Order Logic (Optional/Next Phase)

Since the user explicitly selected "A" (Static Dispatch via Generics) primarily to decouple the *SBE encoding* from the matching loop to improve testing and code taste, extracting Condition Orders is a good secondary step but might be too large for one PR. 
*Self-correction: The spec mentions extracting it. I will write a minimal task to create a `ConditionOrderManager` struct, but if it proves too tightly coupled to `Engine`'s internal `BTreeMap`s and `Slab`s, it might be better handled in a subsequent plan. Let's provide the scaffolding.*

**Files:**
- Create: `mt-engine-core/src/engine/condition.rs`
- Modify: `mt-engine-core/src/engine/mod.rs`

- [ ] **Step 1: Create `ConditionOrderManager`**

Create `mt-engine-core/src/engine/condition.rs`:
```rust
use crate::orders::OrderData;
use crate::types::{OrderId, Price};
use slab::Slab;
use std::collections::BTreeMap;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerNode {
    pub order_idx: usize,
    pub prev: u32,
    pub next: u32,
}
pub const NULL_NODE: u32 = u32::MAX;

pub struct ConditionOrderManager {
    pub condition_order_store: Slab<OrderData>,
    pub trigger_node_pool: Slab<TriggerNode>,
    pub stop_buy_triggers: BTreeMap<Price, u32>,
    pub stop_sell_triggers: BTreeMap<Price, u32>,
    pub tp_buy_triggers: BTreeMap<Price, u32>,
    pub tp_sell_triggers: BTreeMap<Price, u32>,
    pub pending_stop_map: FxHashMap<OrderId, (usize, u32)>,
}

impl ConditionOrderManager {
    pub fn new() -> Self {
        Self {
            condition_order_store: Slab::with_capacity(1024),
            trigger_node_pool: Slab::with_capacity(1024),
            stop_buy_triggers: BTreeMap::new(),
            stop_sell_triggers: BTreeMap::new(),
            tp_buy_triggers: BTreeMap::new(),
            tp_sell_triggers: BTreeMap::new(),
            pending_stop_map: FxHashMap::with_capacity_and_hasher(1024, Default::default()),
        }
    }
}
```

- [ ] **Step 2: Move fields from `Engine`**

In `mt-engine-core/src/engine/mod.rs`, replace the 7 fields with `pub cond_manager: ConditionOrderManager`.
Update `Engine::new()` to initialize `cond_manager: ConditionOrderManager::new()`.

- [ ] **Step 3: Update `register_condition_order` and `unregister_condition_trigger`**

Move these methods to `impl ConditionOrderManager` (passing `ltp` as an argument). Update `Engine` to call `self.cond_manager.register_condition_order(...)`.

- [ ] **Step 4: Commit**

```bash
git add mt-engine-core/src/engine/condition.rs mt-engine-core/src/engine/mod.rs
git commit -m "refactor(engine): extract ConditionOrderManager state"
```
