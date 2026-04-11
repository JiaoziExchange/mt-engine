pub mod condition;
pub mod events;
pub mod sbe_listener;

pub use condition::{ConditionOrderManager, TriggerNode, NULL_NODE};
pub use events::OrderEventListener;
pub use sbe_listener::SbeEncoderListener;

use crate::book::backend::sparse::SparseBackend;
use crate::book::backend::OrderBookBackend;
use crate::orders::{OrderData, RestingOrder};
use crate::outcome::{CommandFailure, CommandOutcome, CommandReport, OrderStatus};
#[cfg(feature = "rkyv")]
use crate::snapshot::rkyv_util::SlabWrapper;
#[cfg(any(feature = "snapshot", feature = "rkyv", feature = "serde"))]
use crate::snapshot::SnapshotModel;
use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::control_message_codec::decoder::ControlMessageDecoder;
use mt_engine::control_op::ControlOp;
use mt_engine::order_amend_codec::decoder::OrderAmendDecoder;
use mt_engine::order_cancel_codec::decoder::OrderCancelDecoder;
use mt_engine::order_submit_codec::decoder::OrderSubmitDecoder;
use mt_engine::order_submit_gtd_codec::decoder::OrderSubmitGtdDecoder;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;
#[cfg(all(feature = "rkyv", feature = "snapshot"))]
use rkyv::ser::Serializer;
#[cfg(feature = "rkyv")]
use rkyv::with::DeserializeWith;
#[cfg(feature = "rkyv")]
use rkyv::{archived_root, Deserialize};
#[cfg(feature = "serde")]
use serde as _;

#[cfg(feature = "snapshot")]
use crate::snapshot::SnapshotConfig;

/// Matching Engine - Single-threaded, high-performance state machine
/// Option B: Zero-allocation pipeline & direct SBE encoding
pub struct Engine<B: OrderBookBackend = SparseBackend, L: OrderEventListener = ()> {
    pub backend: B,
    pub(crate) last_sequence_number: SequenceNumber,
    pub(crate) last_timestamp: Timestamp,
    pub(crate) trade_id_seq: u64,

    /// Unified event listener (replaces direct response_buffer)
    pub listener: L,

    /// Last Trade Price (LTP)
    pub(crate) ltp: Price,

    /// Condition Order Manager (SL/TP Orders)
    pub(crate) cond_manager: ConditionOrderManager,

    #[cfg(feature = "snapshot")]
    pub snapshot_config: Option<SnapshotConfig>,
    #[cfg(feature = "snapshot")]
    pub(crate) uncommitted_commands: u64,
    #[cfg(feature = "snapshot")]
    pub(crate) last_snapshot_ts: u64,
    #[cfg(feature = "snapshot")]
    pub(crate) snapshotting_pid: libc::pid_t,
    /// Last assigned Order ID (used for monotonicity check)
    pub(crate) last_order_id: OrderId,
    /// Engine halt flag
    pub halted: bool,
}

impl<B: OrderBookBackend, L: OrderEventListener> Engine<B, L> {
    pub fn new(backend: B, listener: L) -> Self {
        Self {
            backend,
            last_sequence_number: SequenceNumber(0),
            last_timestamp: Timestamp(0),
            trade_id_seq: 0,
            listener,
            ltp: Price(0),
            cond_manager: ConditionOrderManager::new(),
            #[cfg(feature = "snapshot")]
            snapshot_config: None,
            #[cfg(feature = "snapshot")]
            uncommitted_commands: 0,
            #[cfg(feature = "snapshot")]
            last_snapshot_ts: 0,
            #[cfg(feature = "snapshot")]
            snapshotting_pid: 0,
            last_order_id: OrderId(0),
            halted: false,
        }
    }

    #[inline(always)]
    pub fn get_last_seq(&self) -> SequenceNumber {
        self.last_sequence_number
    }

    #[inline(always)]
    pub fn get_last_ts(&self) -> Timestamp {
        self.last_timestamp
    }

    #[inline(always)]
    pub fn get_last_order_id(&self) -> OrderId {
        self.last_order_id
    }

    #[inline(always)]
    pub fn get_ltp(&self) -> Price {
        self.ltp
    }

    /// Get total depth for a specific price level
    #[inline(always)]
    pub fn get_depth(&self, side: Side, price: Price) -> u64 {
        self.backend.get_total_depth(side, price)
    }

    /// Pre-check if FOK (Fill-Or-Kill) liquidity is sufficient
    fn check_fok_fillable(
        &self,
        taker_side: Side,
        taker_price: Price,
        taker_qty: Quantity,
    ) -> bool {
        let opp_side = match taker_side {
            Side::buy => Side::sell,
            Side::sell => Side::buy,
            _ => return false,
        };
        let depth = self.backend.get_total_depth(opp_side, taker_price);
        depth >= taker_qty.0
    }

    /// Execute order submission
    pub fn execute_submit(&mut self, decoder: &OrderSubmitDecoder) -> CommandOutcome<'_> {
        if self.halted {
            return CommandOutcome::Rejected(CommandFailure::SystemHalted);
        }
        // Strictly enforced: Order IDs must be unique and monotonically increasing (O(1) fast check)
        let order_id = OrderId(decoder.order_id());
        if order_id <= self.last_order_id {
            return CommandOutcome::Rejected(CommandFailure::DuplicateOrderId);
        }
        self.last_order_id = order_id;
        let seq = SequenceNumber(decoder.sequence_number());
        let ts = Timestamp(decoder.timestamp());

        if seq <= self.last_sequence_number {
            return CommandOutcome::Rejected(CommandFailure::SequenceGap);
        }
        self.last_sequence_number = seq;
        self.last_timestamp = ts;

        let tif = decoder.time_in_force();
        let quantity = decoder.quantity();
        let side = decoder.side();
        let price = Price(decoder.price());
        let order_type = decoder.order_type();

        if !self.backend.validate_price(price) {
            return CommandOutcome::Rejected(CommandFailure::InvalidPrice);
        }

        if tif == TimeInForce::fok && !self.check_fok_fillable(side, price, Quantity(quantity)) {
            return CommandOutcome::Rejected(CommandFailure::LiquidityInsufficient);
        }

        let mut taker_order = OrderData {
            remaining_qty: Quantity(quantity),
            filled_qty: Quantity(0),
            price,
            side,
            order_type,
            flags: decoder.flags(),
            order_id: OrderId(decoder.order_id()),
            user_id: UserId(decoder.user_id()),
            sequence_number: seq,
            timestamp: ts,
            expiry: Timestamp(0),
            trigger_price: Price(decoder.price()),
            visible_qty: Quantity(quantity),
            peak_size: Quantity(quantity), // Default peak = total
        };

        // [Conditional Order Branch (Stop/TP)]
        if taker_order.is_stop() {
            self.cond_manager
                .register_condition_order(taker_order, self.ltp);
            return CommandOutcome::Applied(CommandReport {
                order_id: taker_order.order_id,
                status: OrderStatus::New,
                timestamp: ts,
                payload: &[],
            });
        }

        let mut offset = 0usize;
        let initial_ltp = self.ltp;
        if let Err(fail) = self.match_order(&mut taker_order, ts, seq, &mut offset) {
            return CommandOutcome::Rejected(fail);
        }

        // [Stop Order Activation Logic (LTP-driven)]
        if self.ltp != initial_ltp {
            self.process_triggered_orders(ts, seq, &mut offset);
        }

        // Handle remaining quantity
        let final_status = if taker_order.is_fully_filled() {
            OrderStatus::Filled
        } else {
            if tif == TimeInForce::ioc {
                if taker_order.filled_qty.0 > 0 {
                    OrderStatus::PartiallyFilled
                } else {
                    OrderStatus::Cancelled
                }
            } else {
                match taker_order.side {
                    Side::buy | Side::sell => {
                        if let Err(fail) = self.add_resting_order(taker_order) {
                            return CommandOutcome::Rejected(fail);
                        }
                        if taker_order.filled_qty.0 > 0 {
                            OrderStatus::PartiallyFilled
                        } else {
                            OrderStatus::New
                        }
                    }
                    _ => return CommandOutcome::Rejected(CommandFailure::InvalidOrder),
                }
            }
        };

        #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
        self.check_snapshot_trigger();

        CommandOutcome::Applied(CommandReport {
            order_id: taker_order.order_id,
            status: final_status,
            timestamp: ts,
            payload: self.listener.get_payload(offset),
        })
    }

    /// Execute order submission with expiry (GTD/GTH)
    pub fn execute_submit_gtd(&mut self, decoder: &OrderSubmitGtdDecoder) -> CommandOutcome<'_> {
        if self.halted {
            return CommandOutcome::Rejected(CommandFailure::SystemHalted);
        }
        let seq = SequenceNumber(decoder.sequence_number());
        let ts = Timestamp(decoder.timestamp());
        if seq <= self.last_sequence_number {
            return CommandOutcome::Rejected(CommandFailure::SequenceGap);
        }
        self.last_sequence_number = seq;
        self.last_timestamp = ts;

        let mut taker_order = OrderData {
            remaining_qty: Quantity(decoder.quantity()),
            filled_qty: Quantity(0),
            price: Price(decoder.price()),
            side: decoder.side(),
            order_type: decoder.order_type(),
            flags: decoder.flags(),
            order_id: OrderId(decoder.order_id()),
            user_id: UserId(decoder.user_id()),
            sequence_number: seq,
            timestamp: ts,
            expiry: Timestamp(decoder.expiry_time()),
            trigger_price: Price(0),
            visible_qty: Quantity(decoder.quantity()),
            peak_size: Quantity(decoder.quantity()),
        };

        if !self.backend.validate_price(taker_order.price) {
            return CommandOutcome::Rejected(CommandFailure::InvalidPrice);
        }

        if taker_order.is_expired(ts) {
            return CommandOutcome::Rejected(CommandFailure::Expired);
        }

        let mut offset = 0usize;

        // [Conditional Order Branch (GTD + SL/TP)]
        if taker_order.is_stop() {
            self.cond_manager
                .register_condition_order(taker_order, self.ltp);
            return CommandOutcome::Applied(CommandReport {
                order_id: taker_order.order_id,
                status: OrderStatus::New,
                timestamp: ts,
                payload: &[],
            });
        }

        if let Err(fail) = self.match_order(&mut taker_order, ts, seq, &mut offset) {
            return CommandOutcome::Rejected(fail);
        }
        let final_status = if taker_order.is_fully_filled() {
            OrderStatus::Filled
        } else {
            if let Err(fail) = self.add_resting_order(taker_order) {
                return CommandOutcome::Rejected(fail);
            }
            if taker_order.filled_qty.0 > 0 {
                OrderStatus::PartiallyFilled
            } else {
                OrderStatus::New
            }
        };

        #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
        self.check_snapshot_trigger();

        CommandOutcome::Applied(CommandReport {
            order_id: taker_order.order_id,
            status: final_status,
            timestamp: ts,
            payload: self.listener.get_payload(offset),
        })
    }

    /// Core matching loop logic - Direct SBE encoding
    fn match_order(
        &mut self,
        taker_order: &mut OrderData,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    ) -> Result<(), CommandFailure> {
        loop {
            if taker_order.is_fully_filled() {
                break;
            }

            // Get best opposite price
            let best_opp_price = match taker_order.side {
                Side::buy => self.backend.best_ask_price(),
                Side::sell => self.backend.best_bid_price(),
                _ => break,
            };

            let Some(opp_price) = best_opp_price else {
                break;
            };

            // Limit order price check
            if taker_order.order_type == OrderType::limit {
                match taker_order.side {
                    Side::buy if taker_order.price < opp_price => break,
                    Side::sell if taker_order.price > opp_price => break,
                    _ => {}
                }
            }

            // [Deferred Check - Post-Only]
            if taker_order.flags.get_post_only() {
                return Err(CommandFailure::PostOnlyViolation);
            }

            let Some(level_idx) = self.backend.get_level(opp_price) else {
                continue;
            };

            // Get Maker index without popping immediately, as we might handle Iceberg orders or failure later
            let Some(maker_idx) = self.backend.pop_from_level(level_idx) else {
                self.backend.remove_empty_level(level_idx);
                continue;
            };

            // [Performance Optimization - Hardware Prefetch]
            self.backend.prefetch_entry(maker_idx);

            let Some(mut maker_order) = self.backend.remove_order(maker_idx) else {
                continue;
            };

            // [Deferred Check - Expiry]
            if maker_order.data.is_expired(ts) {
                // Silently cancel and continue
                if self.backend.level_order_count(level_idx) == 0 {
                    self.backend.remove_empty_level(level_idx);
                }
                continue;
            }

            let mut trade_qty = std::cmp::min(
                taker_order.remaining_qty.0,
                maker_order.data.remaining_qty.0,
            );

            // [Iceberg Order Logic]: A single match cannot exceed the current visible peak
            if maker_order.data.is_iceberg() {
                trade_qty = std::cmp::min(trade_qty, maker_order.data.visible_qty.0);
            }

            if trade_qty == 0 {
                // If Iceberg peak is exhausted, trigger re-queuing and [CONTINUE] matching
                if maker_order.data.is_iceberg() && maker_order.data.visible_qty.0 == 0 {
                    let reload_qty = std::cmp::min(
                        maker_order.data.remaining_qty.0,
                        maker_order.data.peak_size.0,
                    );
                    maker_order.data.visible_qty = Quantity(reload_qty);
                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_back(level_idx, new_maker_idx);
                    continue; // Key: Continue to match the next Maker
                } else {
                    // Normally trade_qty == 0 won't happen for non-Iceberg orders; perform defensive re-insertion and exit
                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_front(level_idx, new_maker_idx);
                    break;
                }
            }

            taker_order.remaining_qty.0 -= trade_qty;
            taker_order.filled_qty.0 += trade_qty;
            maker_order.data.remaining_qty.0 -= trade_qty;
            maker_order.data.filled_qty.0 += trade_qty;

            // Iceberg visible quantity handling
            if maker_order.data.visible_qty.0 > 0 {
                maker_order.data.visible_qty.0 =
                    maker_order.data.visible_qty.0.saturating_sub(trade_qty);
            }

            self.trade_id_seq += 1;

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

            // Update Last Trade Price (LTP)
            self.ltp = opp_price;

            if !maker_order.data.is_fully_filled() {
                // If it's an Iceberg order and Peak is exhausted, re-queue and [CONTINUE] matching
                if maker_order.data.visible_qty.0 == 0 && maker_order.data.is_iceberg() {
                    // Automatically reload the next Peak from remaining_qty
                    let reload_qty = std::cmp::min(
                        maker_order.data.remaining_qty.0,
                        maker_order.data.peak_size.0,
                    );
                    maker_order.data.visible_qty = Quantity(reload_qty);

                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_back(level_idx, new_maker_idx);
                    continue; // Key: Continue matching
                } else {
                    // Normal case (or Peak still has remaining), Taker is exhausted, re-insert to front and exit
                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_front(level_idx, new_maker_idx);
                    break;
                }
            } else if self.backend.level_order_count(level_idx) == 0 {
                self.backend.remove_empty_level(level_idx);
            }
        }
        Ok(())
    }

    /// Execute order cancellation
    #[inline]
    pub fn execute_cancel(&mut self, decoder: &OrderCancelDecoder) -> CommandOutcome<'_> {
        if self.halted {
            return CommandOutcome::Rejected(CommandFailure::SystemHalted);
        }
        let seq = SequenceNumber(decoder.sequence_number());
        let ts = Timestamp(decoder.timestamp());
        if seq <= self.last_sequence_number {
            return CommandOutcome::Rejected(CommandFailure::SequenceGap);
        }
        self.last_sequence_number = seq;
        self.last_timestamp = ts;

        let order_id = OrderId(decoder.order_id());
        if let Some(order_idx) = self.backend.get_order_idx_by_id(order_id) {
            let order_ref = self.backend.get_order(order_idx).expect("Exists");
            let level_idx = order_ref.level_idx;
            self.backend.remove_from_level(level_idx, order_idx);
            self.backend.remove_order(order_idx);
            self.backend.remove_empty_level(level_idx);

            #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
            self.check_snapshot_trigger();

            CommandOutcome::Applied(CommandReport {
                order_id,
                status: OrderStatus::Cancelled,
                timestamp: ts,
                payload: &[],
            })
        } else if let Some((s_idx, node_idx)) = self.cond_manager.pending_stop_map.remove(&order_id)
        {
            // Remove completely from the conditional order pool and sync-clear the trigger index to prevent memory leaks
            if let Some(order) = self.cond_manager.condition_order_store.try_remove(s_idx) {
                self.cond_manager
                    .unregister_condition_trigger(node_idx, &order, self.ltp);
            }

            CommandOutcome::Applied(CommandReport {
                order_id,
                status: OrderStatus::Cancelled,
                timestamp: ts,
                payload: &[],
            })
        } else {
            CommandOutcome::Rejected(CommandFailure::OrderNotFound)
        }
    }

    /// Execute order amendment
    #[inline]
    pub fn execute_amend(&mut self, decoder: &OrderAmendDecoder) -> CommandOutcome<'_> {
        if self.halted {
            return CommandOutcome::Rejected(CommandFailure::SystemHalted);
        }
        let seq = SequenceNumber(decoder.sequence_number());
        let ts = Timestamp(decoder.timestamp());
        if seq <= self.last_sequence_number {
            return CommandOutcome::Rejected(CommandFailure::SequenceGap);
        }
        self.last_sequence_number = seq;
        self.last_timestamp = ts;

        let order_id = OrderId(decoder.order_id());
        let new_price = Price(decoder.new_price());
        let new_qty = Quantity(decoder.new_quantity());

        if let Some(order_idx) = self.backend.get_order_idx_by_id(order_id) {
            let current_order = self.backend.get_order(order_idx).expect("Exists");
            if new_price == current_order.data.price
                && new_qty.0 <= current_order.data.remaining_qty.0
            {
                self.backend.modify_order_qty(order_idx, new_qty);
                CommandOutcome::Applied(CommandReport {
                    order_id,
                    status: OrderStatus::New,
                    timestamp: ts,
                    payload: &[],
                })
            } else {
                let old_level_idx = current_order.level_idx;
                let old_data = current_order.data; // Backup original data for failure rollback
                self.backend.remove_from_level(old_level_idx, order_idx);
                let mut order = self.backend.remove_order(order_idx).expect("Exists");
                self.backend.remove_empty_level(old_level_idx);

                let original_ltp = self.ltp; // Record LTP before amendment
                order.data.price = new_price;
                order.data.remaining_qty = new_qty;

                if order.data.is_expired(ts) {
                    return CommandOutcome::Rejected(CommandFailure::Expired);
                }

                let mut offset = 0usize;
                if let Err(fail) = self.match_order(&mut order.data, ts, seq, &mut offset) {
                    // [Rollback Logic]: If matching errors occur (e.g., PostOnly violation), re-insert original data
                    let _ = self.add_resting_order(old_data);
                    return CommandOutcome::Rejected(fail);
                }

                let final_status = if order.data.is_fully_filled() {
                    OrderStatus::Filled
                } else {
                    if let Err(fail) = self.add_resting_order(order.data) {
                        // [Rollback Logic]: If resting order fails, restore the original order
                        let _ = self.add_resting_order(old_data);
                        return CommandOutcome::Rejected(fail);
                    }
                    if order.data.filled_qty.0 > 0 {
                        OrderStatus::PartiallyFilled
                    } else {
                        OrderStatus::New
                    }
                };

                // [Trigger Check]: If LTP changes, trigger Stop Loss / Take Profit logic
                if self.ltp != original_ltp {
                    self.process_triggered_orders(ts, seq, &mut offset);
                }

                #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
                self.check_snapshot_trigger();

                CommandOutcome::Applied(CommandReport {
                    order_id,
                    status: final_status,
                    timestamp: ts,
                    payload: self.listener.get_payload(offset),
                })
            }
        } else {
            CommandOutcome::Rejected(CommandFailure::OrderNotFound)
        }
    }

    /// Execute system control commands (Shutdown, etc.)
    pub fn execute_control(&mut self, decoder: &ControlMessageDecoder) -> CommandOutcome<'_> {
        let seq = SequenceNumber(decoder.sequence_number());
        let ts = Timestamp(decoder.timestamp());
        if seq <= self.last_sequence_number {
            return CommandOutcome::Rejected(CommandFailure::SequenceGap);
        }
        self.last_sequence_number = seq;
        self.last_timestamp = ts;

        match decoder.control_op() {
            ControlOp::shutdown => {
                #[cfg(feature = "dev")]
                println!("[Dev] Shutdown control received. Triggering snapshot and halting.");

                // 1. Trigger mandatory snapshot (only when snapshot feature is enabled)
                #[cfg(all(feature = "snapshot", feature = "rkyv"))]
                let _ = self.trigger_snapshot_rkyv();

                // 2. Set engine halt flag
                self.halted = true;

                CommandOutcome::Applied(CommandReport {
                    order_id: OrderId(0),
                    status: OrderStatus::Cancelled, // Use Cancelled or custom status to indicate command processed
                    timestamp: ts,
                    payload: &[],
                })
            }
            _ => CommandOutcome::Rejected(CommandFailure::InvalidOrder),
        }
    }

    /// Execute conditional order trigger sequence (SL/TP)
    fn process_triggered_orders(&mut self, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        // Use a loop to handle triggers to prevent stack overflow, as triggers can cause chain reactions.
        // Set a maximum activation depth (e.g., 10 levels) to prevent infinite recursion in extreme cases.
        let mut depth = 0;
        const MAX_TRIGGER_DEPTH: u32 = 10;

        loop {
            if depth >= MAX_TRIGGER_DEPTH {
                // TODO: Handle cascade trigger truncation. If depth exceeds MAX_TRIGGER_DEPTH and stops,
                // remaining triggered orders will stay in the pool until the next price change.
                // Recommendation: Add overflow error blocking or move legacy orders to a separate delayed queue.
                break;
            }
            depth += 1;

            let initial_ltp = self.ltp;

            // 1. Collect and prefetch all triggered order indices (zero-allocation)
            self.cond_manager.collect_triggered_indices(self.ltp);

            if self.cond_manager.trigger_index_buffer.is_empty() {
                break;
            }

            // 2. Activate triggered orders sequentially (data now prefetched to L1)
            for i in 0..self.cond_manager.trigger_index_buffer.len() {
                let s_idx = self.cond_manager.trigger_index_buffer[i];
                if let Some(mut triggered_order) =
                    self.cond_manager.condition_order_store.try_remove(s_idx)
                {
                    // Remove from the mapping table after activation
                    self.cond_manager
                        .pending_stop_map
                        .remove(&triggered_order.order_id);

                    if triggered_order.is_expired(ts) {
                        #[cfg(feature = "dev")]
                        println!(
                            "[Dev] Triggered order {} is already expired at {}, skipping.",
                            triggered_order.order_id.0, ts.0
                        );
                        continue;
                    }

                    // Perform match_order directly after activation
                    let _ = self.match_order(&mut triggered_order, ts, seq, offset);

                    if !triggered_order.is_fully_filled() {
                        if let Err(_e) = self.add_resting_order(triggered_order) {
                            #[cfg(feature = "dev")]
                            println!("[Dev] Cascade trigger add_resting_order failed: {:?}", _e);
                        }
                    }
                }
            }

            // 3. Multi-level cascade trigger
            if self.ltp == initial_ltp {
                break;
            }
        }
    }

    /// Internal logic: Add remaining orders to the order book
    fn add_resting_order(&mut self, order_data: OrderData) -> Result<(), CommandFailure> {
        let level_idx = self
            .backend
            .get_or_create_level(order_data.side, order_data.price);
        let order_idx = self
            .backend
            .insert_order(RestingOrder::new(order_data, level_idx))?;
        self.backend.push_to_level_back(level_idx, order_idx);
        Ok(())
    }

    // ========== Snapshot Core Logic ==========

    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    pub fn to_snapshot(&self) -> SnapshotModel {
        SnapshotModel {
            last_sequence_number: self.last_sequence_number,
            last_timestamp: self.last_timestamp,
            trade_id_seq: self.trade_id_seq,
            ltp: self.ltp,
            last_order_id: self.last_order_id,
            backend: self.backend.transfer_to_sparse(),
            condition_orders: self.cond_manager.condition_order_store.clone(),
        }
    }

    #[cfg(feature = "rkyv")]
    pub fn to_snapshot_rkyv(&self) -> SnapshotModel {
        self.to_snapshot()
    }

    #[cfg(feature = "serde")]
    pub fn from_snapshot(&mut self, model: SnapshotModel) {
        self.last_sequence_number = model.last_sequence_number;
        self.last_timestamp = model.last_timestamp;
        self.trade_id_seq = model.trade_id_seq;
        self.ltp = model.ltp;
        self.backend.import_levels(model.backend.export_levels());

        self.cond_manager.clear();

        for (_, order) in model.condition_orders {
            self.cond_manager.register_condition_order(order, self.ltp);
        }
    }

    #[cfg(all(feature = "snapshot", feature = "rkyv"))]
    pub fn trigger_snapshot_rkyv(&mut self) -> Result<(), String> {
        let ts = self.last_timestamp.0;
        let seq = self.last_sequence_number.0;

        let path = if let Some(config) = &self.snapshot_config {
            config
                .path_template
                .replace("{seq}", &seq.to_string())
                .replace("{ts}", &ts.to_string())
                .replace(".zst", "") // Full compression is generally not recommended for rkyv + mmap
        } else {
            format!("./snapshot_{}.rkyv", seq)
        };

        #[cfg(feature = "dev")]
        println!("[Dev] Rkyv Snapshot triggered via CoW fork: path={}", path);

        unsafe {
            let pid = libc::fork();
            if pid < 0 {
                return Err("Fork failed".to_string());
            }

            if pid == 0 {
                let result = (|| -> Result<(), String> {
                    let snapshot_path = std::path::Path::new(&path);
                    if let Some(parent) = snapshot_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }

                    // Use rkyv for archiving
                    let mut serializer =
                        rkyv::ser::serializers::AllocSerializer::<1048576>::default();
                    let model = SnapshotModel {
                        last_sequence_number: self.last_sequence_number,
                        last_timestamp: self.last_timestamp,
                        trade_id_seq: self.trade_id_seq,
                        ltp: self.ltp,
                        last_order_id: self.last_order_id,
                        backend: self.backend.transfer_to_sparse(),
                        condition_orders: self.cond_manager.condition_order_store.clone(),
                    };

                    Serializer::serialize_value(&mut serializer, &model)
                        .map_err(|e| e.to_string())?;
                    let bytes = serializer.into_serializer().into_inner();

                    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                    Ok(())
                })();

                match result {
                    Ok(_) => libc::_exit(0),
                    Err(e) => {
                        eprintln!("[Snapshot Child] Rkyv Error: {}", e);
                        libc::_exit(1);
                    }
                }
            } else {
                self.snapshotting_pid = pid;
                self.uncommitted_commands = 0;
                self.last_snapshot_ts = ts;
                Ok(())
            }
        }
    }

    #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
    #[inline(always)]
    fn check_snapshot_trigger(&mut self) {
        if let Some(config) = &self.snapshot_config {
            self.uncommitted_commands += 1;
            let time_passed = self.last_timestamp.0.saturating_sub(self.last_snapshot_ts);

            if self.uncommitted_commands >= config.count_interval
                || (config.time_interval_ms > 0 && time_passed >= config.time_interval_ms)
            {
                if self.snapshotting_pid != 0 {
                    let mut status = 0;
                    unsafe {
                        let ret = libc::waitpid(self.snapshotting_pid, &mut status, libc::WNOHANG);
                        if ret == self.snapshotting_pid || ret < 0 {
                            self.snapshotting_pid = 0;
                        } else {
                            return;
                        }
                    }
                }

                #[cfg(feature = "rkyv")]
                let _ = self.trigger_snapshot_rkyv();
                #[cfg(all(not(feature = "rkyv"), feature = "serde"))]
                let _ = self.trigger_snapshot(); // Retain compatibility calls for legacy logic
            }
        }
    }

    #[cfg(feature = "rkyv")]
    pub fn load_snapshot_rkyv(&mut self, path: &str) -> Result<(), String> {
        use ::memmap2::Mmap;
        use std::fs::File;

        let file = File::open(path).map_err(|e| e.to_string())?;
        let mmap = unsafe { Mmap::map(&file).map_err(|e| e.to_string())? };

        let archived = unsafe { archived_root::<SnapshotModel>(&mmap) };

        self.last_sequence_number = archived
            .last_sequence_number
            .deserialize(&mut rkyv::Infallible)
            .unwrap();
        self.last_timestamp = archived
            .last_timestamp
            .deserialize(&mut rkyv::Infallible)
            .unwrap();
        self.trade_id_seq = archived
            .trade_id_seq
            .deserialize(&mut rkyv::Infallible)
            .unwrap();
        self.ltp = archived.ltp.deserialize(&mut rkyv::Infallible).unwrap();
        self.last_order_id = archived
            .last_order_id
            .deserialize(&mut rkyv::Infallible)
            .unwrap();

        self.cond_manager.condition_order_store =
            SlabWrapper::deserialize_with(&archived.condition_orders, &mut rkyv::Infallible)
                .unwrap();
        self.restore_backend_from_archived(&archived.backend)?;

        self.cond_manager.clear(); // Actually we just imported data, let's re-register to rebuild indices

        let orders: Vec<_> = self
            .cond_manager
            .condition_order_store
            .iter()
            .map(|(_, o)| *o)
            .collect();
        for order in orders {
            self.cond_manager.register_condition_order(order, self.ltp);
        }

        Ok(())
    }

    #[cfg(feature = "rkyv")]
    fn restore_backend_from_archived(
        &mut self,
        archived_backend: &rkyv::Archived<crate::book::backend::sparse::SparseBackend>,
    ) -> Result<(), String> {
        self.backend.import_from_archived_sparse(archived_backend);
        Ok(())
    }

    #[cfg(feature = "dev")]
    pub fn dev_print_state(&self) {
        println!("======= [Engine State Observation] =======");
        println!("Last Seq:    {}", self.last_sequence_number.0);
        println!("Last TS:     {}", self.last_timestamp.0);
        println!("LTP:         {}", self.ltp.0);
        println!("Cond Orders: {}", self.cond_manager.len());
        println!("Trade ID:    {}", self.trade_id_seq);
        #[cfg(feature = "serde")]
        println!("Has Config:  {}", self.snapshot_config.is_some());
        #[cfg(feature = "snapshot")]
        {
            println!("Uncommitted: {}", self.uncommitted_commands);
            println!("Child PID:   {}", self.snapshotting_pid);
        }
        println!("==========================================");
    }
}
