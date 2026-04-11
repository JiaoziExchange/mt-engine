pub mod events;
pub use events::OrderEventListener;

pub mod sbe_listener;
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
use mt_engine::order_amend_codec::decoder::OrderAmendDecoder;
use mt_engine::order_cancel_codec::decoder::OrderCancelDecoder;
use mt_engine::order_submit_codec::decoder::OrderSubmitDecoder;
use mt_engine::order_submit_gtd_codec::decoder::OrderSubmitGtdDecoder;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;
#[cfg(feature = "rkyv")]
use rkyv::with::DeserializeWith;
#[cfg(feature = "rkyv")]
use rkyv::{archived_root, ser::Serializer, Deserialize};
use rustc_hash::FxHashMap;
#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use slab::Slab;
use std::collections::BTreeMap;

/// 触发链表节点 (用于侵入式双向链表)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct TriggerNode {
    /// 指向 condition_order_store 的索引
    pub order_idx: usize,
    /// 链表前驱 (u32::MAX 表示空)
    pub prev: u32,
    /// 链表后继 (u32::MAX 表示空)
    pub next: u32,
}

pub const NULL_NODE: u32 = u32::MAX;

#[cfg(feature = "snapshot")]
use crate::snapshot::SnapshotConfig;

/// 撮合引擎 - 单线程、极致性能状态机
/// 方案 B：全链路零分配 & SBE 直接编码
pub struct Engine<B: OrderBookBackend = SparseBackend, L: OrderEventListener = ()> {
    pub backend: B,
    pub(crate) last_sequence_number: SequenceNumber,
    pub(crate) last_timestamp: Timestamp,
    pub(crate) trade_id_seq: u64,

    /// 统一事件监听器 (替代直接的 response_buffer)
    pub listener: L,

    /// 最新成交价 (Last Trade Price)
    pub(crate) ltp: Price,

    /// 条件单暂存区 (SL/TP Orders)
    pub(crate) condition_order_store: Slab<OrderData>,

    /// 预分配触发缓冲区 (避免热路径分配，存储 Slab 索引)
    trigger_index_buffer: Vec<usize>,

    /// 侵入式链表节点池
    pub(crate) trigger_node_pool: Slab<TriggerNode>,

    /// 止损触发池 - 买单 (LTP >= TriggerPrice)
    pub(crate) stop_buy_triggers: BTreeMap<Price, u32>,
    /// 止损触发池 - 卖单 (LTP <= TriggerPrice)
    pub(crate) stop_sell_triggers: BTreeMap<Price, u32>,
    /// 止盈触发池 - 买单 (LTP <= TriggerPrice)
    pub(crate) tp_buy_triggers: BTreeMap<Price, u32>,
    /// 止盈触发池 - 卖单 (LTP >= TriggerPrice)
    pub(crate) tp_sell_triggers: BTreeMap<Price, u32>,

    #[cfg(feature = "snapshot")]
    pub snapshot_config: Option<SnapshotConfig>,
    #[cfg(feature = "snapshot")]
    pub(crate) uncommitted_commands: u64,
    #[cfg(feature = "snapshot")]
    pub(crate) last_snapshot_ts: u64,
    #[cfg(feature = "snapshot")]
    pub(crate) snapshotting_pid: libc::pid_t,
    /// 最后分配的订单 ID (用于单调性校验)
    pub(crate) last_order_id: OrderId,
    /// 待触发条件单 ID 映射 (OrderId -> (OrderStoreIndex, NodeIndex))
    pub(crate) pending_stop_map: FxHashMap<OrderId, (usize, u32)>,
}

impl<B: OrderBookBackend, L: OrderEventListener> Engine<B, L> {
    pub fn new(backend: B, listener: L) -> Self {
        Self {
            backend,
            last_sequence_number: SequenceNumber(0),
            last_timestamp: Timestamp(0),
            trade_id_seq: 0,
            listener,
            stop_buy_triggers: BTreeMap::new(),
            stop_sell_triggers: BTreeMap::new(),
            tp_buy_triggers: BTreeMap::new(),
            tp_sell_triggers: BTreeMap::new(),
            ltp: Price(0),
            condition_order_store: Slab::with_capacity(1024),
            trigger_node_pool: Slab::with_capacity(1024),
            trigger_index_buffer: Vec::with_capacity(64),
            #[cfg(feature = "snapshot")]
            snapshot_config: None,
            #[cfg(feature = "snapshot")]
            uncommitted_commands: 0,
            #[cfg(feature = "snapshot")]
            last_snapshot_ts: 0,
            #[cfg(feature = "snapshot")]
            snapshotting_pid: 0,
            last_order_id: OrderId(0),
            pending_stop_map: FxHashMap::with_capacity_and_hasher(1024, Default::default()),
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

    /// 获取特定价格档位的总深度
    #[inline(always)]
    pub fn get_depth(&self, side: Side, price: Price) -> u64 {
        self.backend.get_total_depth(side, price)
    }

    /// 预检查 FOK 深度是否满足
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

    /// 执行订单提交
    pub fn execute_submit(&mut self, decoder: &OrderSubmitDecoder) -> CommandOutcome<'_> {
        // 严格遵循：订单 ID 必须是不重复且递增的 (O(1) 极速校验)
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

        // 【条件单分支 (Stop/TP)】
        if taker_order.is_stop() {
            self.register_condition_order(taker_order);
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

        // 【Stop 订单激活逻辑 (LTP 驱动)】
        if self.ltp != initial_ltp {
            self.process_triggered_orders(ts, seq, &mut offset);
        }

        // 处理剩余部分
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

    /// 执行带有效期的订单提交 (GTD/GTH)
    pub fn execute_submit_gtd(&mut self, decoder: &OrderSubmitGtdDecoder) -> CommandOutcome<'_> {
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

        // 【条件单分支 (GTD + SL/TP)】
        if taker_order.is_stop() {
            self.register_condition_order(taker_order);
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

    /// 核心撮合循环逻辑封装 - 直接 SBE 编码
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

            // 获取最优对手价
            let best_opp_price = match taker_order.side {
                Side::buy => self.backend.best_ask_price(),
                Side::sell => self.backend.best_bid_price(),
                _ => break,
            };

            let Some(opp_price) = best_opp_price else {
                break;
            };

            // 限价单价格检查
            if taker_order.order_type == OrderType::limit {
                match taker_order.side {
                    Side::buy if taker_order.price < opp_price => break,
                    Side::sell if taker_order.price > opp_price => break,
                    _ => {}
                }
            }

            // 【延迟检查 - Post-Only】
            if taker_order.flags.get_post_only() {
                return Err(CommandFailure::PostOnlyViolation);
            }

            let Some(level_idx) = self.backend.get_level(opp_price) else {
                continue;
            };

            // 获取 Maker 索引但不立刻弹出，因为后面可能需要处理冰山单或是失败
            let Some(maker_idx) = self.backend.pop_from_level(level_idx) else {
                self.backend.remove_empty_level(level_idx);
                continue;
            };

            // 【性能优化 - 硬件预取】
            self.backend.prefetch_entry(maker_idx);

            let Some(mut maker_order) = self.backend.remove_order(maker_idx) else {
                continue;
            };

            // 【延迟检查 - Expiry】
            if maker_order.data.is_expired(ts) {
                // 静默撤单并继续
                if self.backend.level_order_count(level_idx) == 0 {
                    self.backend.remove_empty_level(level_idx);
                }
                continue;
            }

            let mut trade_qty = std::cmp::min(
                taker_order.remaining_qty.0,
                maker_order.data.remaining_qty.0,
            );

            // 【冰山单逻辑】：单次撮合不得超过当前可见峰值
            if maker_order.data.is_iceberg() {
                trade_qty = std::cmp::min(trade_qty, maker_order.data.visible_qty.0);
            }

            if trade_qty == 0 {
                // 如果是冰山单峰值耗尽，触发重新排队并【继续】匹配
                if maker_order.data.is_iceberg() && maker_order.data.visible_qty.0 == 0 {
                    let reload_qty = std::cmp::min(
                        maker_order.data.remaining_qty.0,
                        maker_order.data.peak_size.0,
                    );
                    maker_order.data.visible_qty = Quantity(reload_qty);
                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_back(level_idx, new_maker_idx);
                    continue; // 关键：继续循环匹配下一个 Maker
                } else {
                    // 非冰山单理论上不会出现 trade_qty == 0，做防御性插回并退出
                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_front(level_idx, new_maker_idx);
                    break;
                }
            }

            taker_order.remaining_qty.0 -= trade_qty;
            taker_order.filled_qty.0 += trade_qty;
            maker_order.data.remaining_qty.0 -= trade_qty;
            maker_order.data.filled_qty.0 += trade_qty;

            // 冰山单可见数量处理
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

            // 更新最新成交价
            self.ltp = opp_price;

            if !maker_order.data.is_fully_filled() {
                // 如果是冰山单且 Peak 消耗完，需要重新排队并【继续】匹配
                if maker_order.data.visible_qty.0 == 0 && maker_order.data.is_iceberg() {
                    // 自动从 remaining_qty 中补足下一个 Peak
                    let reload_qty = std::cmp::min(
                        maker_order.data.remaining_qty.0,
                        maker_order.data.peak_size.0,
                    );
                    maker_order.data.visible_qty = Quantity(reload_qty);

                    let new_maker_idx = self.backend.insert_order(maker_order)?;
                    self.backend.push_to_level_back(level_idx, new_maker_idx);
                    continue; // 关键：继续循环
                } else {
                    // 普通情况（或 Peak 还有剩余），说明是 Taker 耗尽，插回队首并退出
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

    /// 执行订单取消
    #[inline]
    pub fn execute_cancel(&mut self, decoder: &OrderCancelDecoder) -> CommandOutcome<'_> {
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
        } else if let Some((s_idx, node_idx)) = self.pending_stop_map.remove(&order_id) {
            // 从条件单池中彻底移除，并同步清理触发索引以防内存泄漏
            if let Some(order) = self.condition_order_store.try_remove(s_idx) {
                self.unregister_condition_trigger(node_idx, &order);
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

    /// 执行订单修改
    #[inline]
    pub fn execute_amend(&mut self, decoder: &OrderAmendDecoder) -> CommandOutcome<'_> {
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
                let old_data = current_order.data; // 备份原数据用于失败回滚
                self.backend.remove_from_level(old_level_idx, order_idx);
                let mut order = self.backend.remove_order(order_idx).expect("Exists");
                self.backend.remove_empty_level(old_level_idx);

                let original_ltp = self.ltp; // 记录改单前的 LTP
                order.data.price = new_price;
                order.data.remaining_qty = new_qty;

                if order.data.is_expired(ts) {
                    return CommandOutcome::Rejected(CommandFailure::Expired);
                }

                let mut offset = 0usize;
                if let Err(fail) = self.match_order(&mut order.data, ts, seq, &mut offset) {
                    // [回滚逻辑]：如果匹配过程出错（如 PostOnly 冲突），将原数据重新挂起
                    let _ = self.add_resting_order(old_data);
                    return CommandOutcome::Rejected(fail);
                }

                let final_status = if order.data.is_fully_filled() {
                    OrderStatus::Filled
                } else {
                    if let Err(fail) = self.add_resting_order(order.data) {
                        // [回滚逻辑]：如果挂单失败，恢复原单
                        let _ = self.add_resting_order(old_data);
                        return CommandOutcome::Rejected(fail);
                    }
                    if order.data.filled_qty.0 > 0 {
                        OrderStatus::PartiallyFilled
                    } else {
                        OrderStatus::New
                    }
                };

                // [触发补全]：如果 LTP 发生变化，触发止损/止盈 logic
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

    /// 执行条件单触发序列 (SL/TP)
    fn process_triggered_orders(&mut self, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        // 由于触发可能导致连环反应，使用循环处理以防栈溢出。
        // 设置最大激活深度（如 10 级）防止极端情况下的无限递归触发。
        let mut depth = 0;
        const MAX_TRIGGER_DEPTH: u32 = 10;

        loop {
            if depth >= MAX_TRIGGER_DEPTH {
                // TODO: 级联触发截断遗留问题。如果触发深度超过 MAX_TRIGGER_DEPTH 而中断，
                // 剩余应触发的条件单将留在池中直到下一次价格变动。
                // 建议：加入超限报错拦截，或将遗留单放回独立的延后队列处理。
                break;
            }
            depth += 1;

            let initial_ltp = self.ltp;
            self.trigger_index_buffer.clear();

            // 1. 搜集并预取所有触发的条件单索引 (零分配)

            // Buy Stop (LTP >= Trigger): 从最小值开始触发
            while let Some(&first_price) = self.stop_buy_triggers.keys().next() {
                if first_price > self.ltp {
                    break;
                }
                if let Some(head_node_idx) = self.stop_buy_triggers.remove(&first_price) {
                    self.collect_and_recycle_trigger_list(head_node_idx);
                }
            }

            // Buy TP (LTP <= Trigger): 从最大值开始触发
            while let Some(&last_price) = self.tp_buy_triggers.keys().next_back() {
                if last_price < self.ltp {
                    break;
                }
                if let Some(head_node_idx) = self.tp_buy_triggers.remove(&last_price) {
                    self.collect_and_recycle_trigger_list(head_node_idx);
                }
            }

            // Sell Stop (LTP <= Trigger): 从最大值开始触发
            while let Some(&last_price) = self.stop_sell_triggers.keys().next_back() {
                if last_price < self.ltp {
                    break;
                }
                if let Some(head_node_idx) = self.stop_sell_triggers.remove(&last_price) {
                    self.collect_and_recycle_trigger_list(head_node_idx);
                }
            }

            // Sell TP (LTP >= Trigger): 从最小值开始触发
            while let Some(&first_price) = self.tp_sell_triggers.keys().next() {
                if first_price > self.ltp {
                    break;
                }
                if let Some(head_node_idx) = self.tp_sell_triggers.remove(&first_price) {
                    self.collect_and_recycle_trigger_list(head_node_idx);
                }
            }

            if self.trigger_index_buffer.is_empty() {
                break;
            }

            // 2. 依次激活条件单 (此时数据已预取至 L1)
            for i in 0..self.trigger_index_buffer.len() {
                let s_idx = self.trigger_index_buffer[i];
                if let Some(mut triggered_order) = self.condition_order_store.try_remove(s_idx) {
                    // 激活后从映射表中移除
                    self.pending_stop_map.remove(&triggered_order.order_id);

                    if triggered_order.is_expired(ts) {
                        #[cfg(feature = "dev")]
                        println!(
                            "[Dev] Triggered order {} is already expired at {}, skipping.",
                            triggered_order.order_id.0, ts.0
                        );
                        continue;
                    }

                    // 激活后直接进行 match_order
                    let _ = self.match_order(&mut triggered_order, ts, seq, offset);

                    if !triggered_order.is_fully_filled() {
                        if let Err(_e) = self.add_resting_order(triggered_order) {
                            #[cfg(feature = "dev")]
                            println!("[Dev] Cascade trigger add_resting_order failed: {:?}", _e);
                        }
                    }
                }
            }

            // 3. 多级联动触发
            if self.ltp == initial_ltp {
                break;
            }
        }
    }

    /// 辅助逻辑：遍历链表，收集索引并回收节点
    fn collect_and_recycle_trigger_list(&mut self, head_node_idx: u32) {
        let mut cur = head_node_idx;
        while cur != NULL_NODE {
            let node = if let Some(n) = self.trigger_node_pool.get(cur as usize) {
                *n
            } else {
                break;
            };

            self.prefetch_condition_order(node.order_idx);
            self.trigger_index_buffer.push(node.order_idx);

            let next = node.next;
            // 物理回收触发节点到 Slab 池
            self.trigger_node_pool.remove(cur as usize);
            cur = next;
        }
    }

    #[inline(always)]
    fn prefetch_condition_order(&self, idx: usize) {
        if let Some(_entry) = self.condition_order_store.get(idx) {
            // 安全优化：使用 x86_64 硬件预取指令将数据载入 L1 Cache。
            #[cfg(target_arch = "x86_64")]
            unsafe {
                use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
                _mm_prefetch(_entry as *const _ as *const i8, _MM_HINT_T0);
            }
            #[cfg(target_arch = "aarch64")]
            unsafe {
                // aarch64 预取指令：pldl1keep (Read, L1, Keep)
                core::arch::asm!("prfm pldl1keep, [{0}]", in(reg) _entry);
            }
        }
    }

    /// 内部逻辑：注册条件单到触发池
    fn register_condition_order(&mut self, order: OrderData) {
        let order_id = order.order_id;
        let trigger_price = order.trigger_price;
        let side = order.side;

        let idx = self.condition_order_store.insert(order);
        let node_idx = self.trigger_node_pool.insert(TriggerNode {
            order_idx: idx,
            prev: NULL_NODE,
            next: NULL_NODE,
        }) as u32;

        self.pending_stop_map.insert(order_id, (idx, node_idx));

        let triggers = match side {
            Side::buy => {
                if trigger_price >= self.ltp {
                    &mut self.stop_buy_triggers
                } else {
                    &mut self.tp_buy_triggers
                }
            }
            Side::sell => {
                if trigger_price <= self.ltp {
                    &mut self.stop_sell_triggers
                } else {
                    &mut self.tp_sell_triggers
                }
            }
            _ => return,
        };

        let entry = triggers.entry(trigger_price).or_insert(NULL_NODE);
        if *entry != NULL_NODE {
            // 将新节点插在链表头部
            self.trigger_node_pool[node_idx as usize].next = *entry;
            self.trigger_node_pool[*entry as usize].prev = node_idx;
        }
        *entry = node_idx;
    }

    /// 内部逻辑：从触发池中卸载条件单 (O(1) 复杂度)
    fn unregister_condition_trigger(&mut self, node_idx: u32, order: &OrderData) {
        let node = if let Some(n) = self.trigger_node_pool.get(node_idx as usize) {
            *n
        } else {
            return;
        };

        let prev = node.prev;
        let next = node.next;

        if prev != NULL_NODE {
            self.trigger_node_pool[prev as usize].next = next;
        } else {
            // 如果是头节点，需要更新 BTreeMap 中的头指针
            let triggers = match order.side {
                Side::buy => {
                    if order.trigger_price >= self.ltp {
                        &mut self.stop_buy_triggers
                    } else {
                        &mut self.tp_buy_triggers
                    }
                }
                Side::sell => {
                    if order.trigger_price <= self.ltp {
                        &mut self.stop_sell_triggers
                    } else {
                        &mut self.tp_sell_triggers
                    }
                }
                _ => return,
            };

            if next == NULL_NODE {
                triggers.remove(&order.trigger_price);
            } else {
                triggers.insert(order.trigger_price, next);
            }
        }

        if next != NULL_NODE {
            self.trigger_node_pool[next as usize].prev = prev;
        }

        // 回收节点到 Slab 池
        self.trigger_node_pool.remove(node_idx as usize);
    }

    /// 内部逻辑：将剩余订单加入下单簿
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

    // ========== 快照 (Snapshot) 核心逻辑 ==========

    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    pub fn to_snapshot(&self) -> SnapshotModel {
        SnapshotModel {
            last_sequence_number: self.last_sequence_number,
            last_timestamp: self.last_timestamp,
            trade_id_seq: self.trade_id_seq,
            ltp: self.ltp,
            last_order_id: self.last_order_id,
            backend: self.backend.transfer_to_sparse(),
            condition_orders: self.condition_order_store.clone(),
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

        self.condition_order_store.clear();
        self.stop_buy_triggers.clear();
        self.stop_sell_triggers.clear();
        self.tp_buy_triggers.clear();
        self.tp_sell_triggers.clear();

        for (_, order) in model.condition_orders {
            self.register_condition_order(order);
        }
    }

    #[cfg(feature = "rkyv")]
    pub fn trigger_snapshot_rkyv(&mut self) -> Result<(), String> {
        let ts = self.last_timestamp.0;
        let seq = self.last_sequence_number.0;

        let path = if let Some(config) = &self.snapshot_config {
            config
                .path_template
                .replace("{seq}", &seq.to_string())
                .replace("{ts}", &ts.to_string())
                .replace(".zst", "") // rkyv + mmap 通常不建议全量压缩
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

                    // 使用 rkyv 进行归档
                    let mut serializer =
                        rkyv::ser::serializers::AllocSerializer::<1048576>::default();
                    let model = SnapshotModel {
                        last_sequence_number: self.last_sequence_number,
                        last_timestamp: self.last_timestamp,
                        trade_id_seq: self.trade_id_seq,
                        ltp: self.ltp,
                        last_order_id: self.last_order_id,
                        backend: self.backend.transfer_to_sparse(),
                        condition_orders: self.condition_order_store.clone(),
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
                let _ = self.trigger_snapshot(); // 保留对原有逻辑的兼容调用
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

        self.condition_order_store =
            SlabWrapper::deserialize_with(&archived.condition_orders, &mut rkyv::Infallible)
                .unwrap();
        self.restore_backend_from_archived(&archived.backend)?;

        self.stop_buy_triggers.clear();
        self.stop_sell_triggers.clear();
        self.tp_buy_triggers.clear();
        self.tp_sell_triggers.clear();
        self.pending_stop_map.clear();

        let orders: Vec<_> = self.condition_order_store.iter().map(|(_, o)| *o).collect();
        for order in orders {
            self.register_condition_order(order);
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
        println!("Cond Orders: {}", self.condition_order_store.len());
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

