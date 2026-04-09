use crate::book::backend::sparse::SparseBackend;
use crate::book::backend::OrderBookBackend;
use crate::orders::{OrderData, RestingOrder};
use crate::outcome::{CommandFailure, CommandOutcome, CommandReport, OrderStatus};
use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::message_header_codec;
use mt_engine::order_amend_codec::decoder::OrderAmendDecoder;
use mt_engine::order_cancel_codec::decoder::OrderCancelDecoder;
use mt_engine::order_submit_codec::decoder::OrderSubmitDecoder;
use mt_engine::order_submit_gtd_codec::decoder::OrderSubmitGtdDecoder;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;
use mt_engine::trade_codec;
use mt_engine::WriteBuf;
use slab::Slab;
use std::collections::BTreeMap;

#[cfg(feature = "snapshot")]
use crate::snapshot::{SnapshotConfig, SnapshotModel};
#[cfg(feature = "snapshot")]
use std::io::Write;

/// 撮合引擎 - 单线程、极致性能状态机
/// 方案 B：全链路零分配 & SBE 直接编码
pub struct Engine<'a, B: OrderBookBackend = SparseBackend> {
    pub backend: B,
    last_sequence_number: SequenceNumber,
    last_timestamp: Timestamp,
    trade_id_seq: u64,

    /// 用户提供的响应缓冲区 (Zero-Allocation & External Management)
    response_buffer: &'a mut [u8],

    /// 最新成交价 (Last Trade Price)
    ltp: Price,

    /// 条件单暂存区 (SL/TP Orders)
    condition_order_store: Slab<OrderData>,

    /// 预分配触发缓冲区 (避免热路径分配，存储 Slab 索引)
    trigger_index_buffer: Vec<usize>,

    /// 止损触发池 - 买单 (LTP >= TriggerPrice)
    stop_buy_triggers: BTreeMap<Price, Vec<usize>>,
    /// 止损触发池 - 卖单 (LTP <= TriggerPrice)
    stop_sell_triggers: BTreeMap<Price, Vec<usize>>,
    /// 止盈触发池 - 买单 (LTP <= TriggerPrice)
    tp_buy_triggers: BTreeMap<Price, Vec<usize>>,
    /// 止盈触发池 - 卖单 (LTP >= TriggerPrice)
    tp_sell_triggers: BTreeMap<Price, Vec<usize>>,

    #[cfg(feature = "snapshot")]
    pub snapshot_config: Option<SnapshotConfig>,
    #[cfg(feature = "snapshot")]
    uncommitted_commands: u64,
    #[cfg(feature = "snapshot")]
    last_snapshot_ts: u64,
    #[cfg(feature = "snapshot")]
    snapshotting_pid: libc::pid_t,
}

impl<'a, B: OrderBookBackend> Engine<'a, B> {
    pub fn new(backend: B, buffer: &'a mut [u8]) -> Self {
        Self {
            backend,
            last_sequence_number: SequenceNumber(0),
            last_timestamp: Timestamp(0),
            trade_id_seq: 0,
            response_buffer: buffer,
            stop_buy_triggers: BTreeMap::new(),
            stop_sell_triggers: BTreeMap::new(),
            tp_buy_triggers: BTreeMap::new(),
            tp_sell_triggers: BTreeMap::new(),
            ltp: Price(0),
            condition_order_store: Slab::with_capacity(1024),
            trigger_index_buffer: Vec::with_capacity(64),
            #[cfg(feature = "snapshot")]
            snapshot_config: None,
            #[cfg(feature = "snapshot")]
            uncommitted_commands: 0,
            #[cfg(feature = "snapshot")]
            last_snapshot_ts: 0,
            #[cfg(feature = "snapshot")]
            snapshotting_pid: 0,
        };

        engine
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
                        self.add_resting_order(taker_order);
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
            payload: &self.response_buffer[..offset],
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
            self.add_resting_order(taker_order);
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
            payload: &self.response_buffer[..offset],
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

            let trade_qty = std::cmp::min(
                taker_order.remaining_qty.0,
                maker_order.data.remaining_qty.0,
            );

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

            // 编码 Trade 报告... (保持原有逻辑)
            let trade_offset = *offset;
            let trade_buf = WriteBuf::new(self.response_buffer);
            let trade_encoder = trade_codec::encoder::TradeEncoder::default().wrap(
                trade_buf,
                trade_offset + message_header_codec::ENCODED_LENGTH,
            );
            let mut header_encoder = trade_encoder.header(trade_offset);

            header_encoder.block_length(trade_codec::SBE_BLOCK_LENGTH);
            header_encoder.template_id(trade_codec::SBE_TEMPLATE_ID);
            header_encoder.schema_id(mt_engine::SBE_SCHEMA_ID);
            header_encoder.version(mt_engine::SBE_SCHEMA_VERSION);

            // 安全优化：使用 unwrap_unchecked 移除热路径分支判断。
            // 理由：SBE 结构设计保证此处 header 后面紧跟 body，故 parent() 必为 Some。
            let mut trade_encoder = unsafe { header_encoder.parent().unwrap_unchecked() };
            trade_encoder.trade_id(self.trade_id_seq);
            trade_encoder.maker_order_id(maker_order.data.order_id.0);
            trade_encoder.taker_order_id(taker_order.order_id.0);
            trade_encoder.side(taker_order.side);
            trade_encoder.price(opp_price.0);
            trade_encoder.quantity(trade_qty);
            trade_encoder.timestamp(ts.0);
            trade_encoder.sequence_number(seq.0);

            *offset +=
                message_header_codec::ENCODED_LENGTH + trade_codec::SBE_BLOCK_LENGTH as usize;

            // 更新最新成交价
            self.ltp = opp_price;

            if !maker_order.data.is_fully_filled() {
                // 如果是冰山单且 Peak 消耗完，需要重新排队
                if maker_order.data.visible_qty.0 == 0 && maker_order.data.is_iceberg() {
                    // 自动从 remaining_qty 中补足下一个 Peak
                    let reload_qty = std::cmp::min(
                        maker_order.data.remaining_qty.0,
                        maker_order.data.peak_size.0,
                    );
                    maker_order.data.visible_qty = Quantity(reload_qty);

                    let new_maker_idx = self.backend.insert_order(maker_order);
                    self.backend.push_to_level_back(level_idx, new_maker_idx);
                } else {
                    // 普通情况（或 Peak 还有剩余），插回队首保持优先级
                    let new_maker_idx = self.backend.insert_order(maker_order);
                    self.backend.push_to_level_front(level_idx, new_maker_idx);
                }
                break;
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
                self.backend.remove_from_level(old_level_idx, order_idx);
                let mut order = self.backend.remove_order(order_idx).expect("Exists");
                self.backend.remove_empty_level(old_level_idx);
                order.data.price = new_price;
                order.data.remaining_qty = new_qty;

                let mut offset = 0usize;
                if let Err(fail) = self.match_order(&mut order.data, ts, seq, &mut offset) {
                    return CommandOutcome::Rejected(fail);
                }

                let final_status = if order.data.is_fully_filled() {
                    OrderStatus::Filled
                } else {
                    self.add_resting_order(order.data);
                    if order.data.filled_qty.0 > 0 {
                        OrderStatus::PartiallyFilled
                    } else {
                        OrderStatus::New
                    }
                };

                #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
                self.check_snapshot_trigger();

                CommandOutcome::Applied(CommandReport {
                    order_id,
                    status: final_status,
                    timestamp: ts,
                    payload: &self.response_buffer[..offset],
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
                if let Some(mut idxs) = self.stop_buy_triggers.remove(&first_price) {
                    for &idx in &idxs {
                        self.prefetch_condition_order(idx);
                    }
                    self.trigger_index_buffer.append(&mut idxs);
                }
            }

            // Buy TP (LTP <= Trigger): 从最大值开始触发
            while let Some(&last_price) = self.tp_buy_triggers.keys().next_back() {
                if last_price < self.ltp {
                    break;
                }
                if let Some(mut idxs) = self.tp_buy_triggers.remove(&last_price) {
                    for &idx in &idxs {
                        self.prefetch_condition_order(idx);
                    }
                    self.trigger_index_buffer.append(&mut idxs);
                }
            }

            // Sell Stop (LTP <= Trigger): 从最大值开始触发
            while let Some(&last_price) = self.stop_sell_triggers.keys().next_back() {
                if last_price < self.ltp {
                    break;
                }
                if let Some(mut idxs) = self.stop_sell_triggers.remove(&last_price) {
                    for &idx in &idxs {
                        self.prefetch_condition_order(idx);
                    }
                    self.trigger_index_buffer.append(&mut idxs);
                }
            }

            // Sell TP (LTP >= Trigger): 从最小值开始触发
            while let Some(&first_price) = self.tp_sell_triggers.keys().next() {
                if first_price > self.ltp {
                    break;
                }
                if let Some(mut idxs) = self.tp_sell_triggers.remove(&first_price) {
                    for &idx in &idxs {
                        self.prefetch_condition_order(idx);
                    }
                    self.trigger_index_buffer.append(&mut idxs);
                }
            }

            if self.trigger_index_buffer.is_empty() {
                break;
            }

            // 2. 依次激活条件单 (此时数据已预取至 L1)
            for i in 0..self.trigger_index_buffer.len() {
                let s_idx = self.trigger_index_buffer[i];
                if let Some(mut triggered_order) = self.condition_order_store.try_remove(s_idx) {
                    // 激活后直接进行 match_order
                    let _ = self.match_order(&mut triggered_order, ts, seq, offset);

                    if !triggered_order.is_fully_filled() {
                        self.add_resting_order(triggered_order);
                    }
                }
            }

            // 3. 多级联动触发
            if self.ltp == initial_ltp {
                break;
            }
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
        }
    }

    /// 内部逻辑：注册条件单到触发池
    fn register_condition_order(&mut self, order: OrderData) {
        let idx = self.condition_order_store.insert(order);
        match order.side {
            Side::buy => {
                if order.trigger_price >= self.ltp {
                    self.stop_buy_triggers
                        .entry(order.trigger_price)
                        .or_default()
                        .push(idx);
                } else {
                    self.tp_buy_triggers
                        .entry(order.trigger_price)
                        .or_default()
                        .push(idx);
                }
            }
            Side::sell => {
                if order.trigger_price <= self.ltp {
                    self.stop_sell_triggers
                        .entry(order.trigger_price)
                        .or_default()
                        .push(idx);
                } else {
                    self.tp_sell_triggers
                        .entry(order.trigger_price)
                        .or_default()
                        .push(idx);
                }
            }
            _ => {
                self.condition_order_store.remove(idx);
            }
        }
    }

    /// 内部逻辑：将剩余订单加入下单簿
    fn add_resting_order(&mut self, order_data: OrderData) {
        let level_idx = self
            .backend
            .get_or_create_level(order_data.side, order_data.price);
        let order_idx = self
            .backend
            .insert_order(RestingOrder::new(order_data, level_idx));
        self.backend.push_to_level_back(level_idx, order_idx);
    }

    // ========== 快照 (Snapshot) 核心逻辑 ==========

    #[cfg(feature = "snapshot")]
    pub fn to_snapshot(&self) -> SnapshotModel {
        SnapshotModel {
            last_sequence_number: self.last_sequence_number,
            last_timestamp: self.last_timestamp,
            trade_id_seq: self.trade_id_seq,
            ltp: self.ltp,
            levels: self.backend.export_levels(),
            condition_orders: self
                .condition_order_store
                .iter()
                .map(|(_, o)| *o)
                .collect(),
        }
    }

    #[cfg(feature = "snapshot")]
    pub fn from_snapshot(&mut self, model: SnapshotModel) {
        self.last_sequence_number = model.last_sequence_number;
        self.last_timestamp = model.last_timestamp;
        self.trade_id_seq = model.trade_id_seq;
        self.ltp = model.ltp;
        self.backend.import_levels(model.levels);

        self.condition_order_store.clear();
        self.stop_buy_triggers.clear();
        self.stop_sell_triggers.clear();
        self.tp_buy_triggers.clear();
        self.tp_sell_triggers.clear();

        for order in model.condition_orders {
            self.register_condition_order(order);
        }
    }

    /// 触发快照 (Fork-based CoW)
    /// 仅在开启 snapshot 特性且 backend 支持导出且非 dense-node 时有效
    #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
    pub fn trigger_snapshot(&mut self) -> Result<(), String> {
        let ts = self.last_timestamp.0;
        let seq = self.last_sequence_number.0;

        let path = if let Some(config) = &self.snapshot_config {
            config
                .path_template
                .replace("{seq}", &seq.to_string())
                .replace("{ts}", &ts.to_string())
        } else {
            format!("./snapshot_{}.bin.zst", seq)
        };

        let compress = self
            .snapshot_config
            .as_ref()
            .map(|c| c.compress)
            .unwrap_or(true);

        unsafe {
            let pid = libc::fork();
            if pid == 0 {
                // 子进程逻辑：在子进程中完成数据搜集，彻底消灭对父进程热路径的干扰
                
                // 1. 设置 CPU 亲和性 (可选，仅 Linux 支持)
                #[cfg(target_os = "linux")]
                if let Ok(cpu_id_str) = std::env::var("SNAPSHOT_CHILD_CPU_ID") {
                    if let Ok(cpu_id) = cpu_id_str.parse::<usize>() {
                        let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
                        libc::CPU_SET(cpu_id, &mut cpuset);
                        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpuset);
                    }
                }

                // 2. 搜集数据构建模型 (在子进程执行)
                let model = SnapshotModel {
                    last_sequence_number: self.last_sequence_number,
                    last_timestamp: self.last_timestamp,
                    trade_id_seq: self.trade_id_seq,
                    ltp: self.ltp,
                    levels: self.backend.export_levels(),
                    condition_orders: {
                        let mut v = Vec::with_capacity(self.condition_order_store.len());
                        for (_, o) in self.condition_order_store.iter() {
                            v.push(*o);
                        }
                        v
                    },
                };

                // 3. 序列化
                let serialized = bincode::serialize(&model).unwrap();

                // 4. 确保目录存在并写入
                let snapshot_path = std::path::Path::new(&path);
                if let Some(parent) = snapshot_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                let result = std::fs::File::create(&path).and_then(|file| {
                    if compress {
                        let mut encoder = zstd::Encoder::new(file, 3)?;
                        encoder.write_all(&serialized)?;
                        encoder.finish()?;
                    } else {
                        let mut writer = std::io::BufWriter::new(file);
                        writer.write_all(&serialized)?;
                    }
                    Ok(())
                });

                if let Err(e) = result {
                    eprintln!("[Snapshot Child] Failed to save snapshot: {}", e);
                    libc::_exit(1);
                }

                libc::_exit(0);
            } else if pid < 0 {
                return Err("Fork failed".to_string());
            } else {
                // 父进程：记录子进程 PID 供状态检查
                self.snapshotting_pid = pid;
            }
        }

        self.uncommitted_commands = 0;
        self.last_snapshot_ts = ts;
        Ok(())
    }

    #[cfg(all(feature = "snapshot", not(feature = "dense-node")))]
    #[inline(always)]
    fn check_snapshot_trigger(&mut self) {
        if let Some(config) = &self.snapshot_config {
            self.uncommitted_commands += 1;
            let time_passed = self
                .last_timestamp
                .0
                .saturating_sub(self.last_snapshot_ts);

            if self.uncommitted_commands >= config.count_interval
                || (config.time_interval_ms > 0 && time_passed >= config.time_interval_ms)
            {
                // 1. 只有在准备触发新快照时，才检查上一个子进程是否结束
                if self.snapshotting_pid != 0 {
                    let mut status = 0;
                    unsafe {
                        let ret = libc::waitpid(self.snapshotting_pid, &mut status, libc::WNOHANG);
                        if ret == self.snapshotting_pid || ret < 0 {
                            // 子进程结束或出错
                            self.snapshotting_pid = 0;
                        } else {
                            // 仍在运行，跳过本次触发，等待下一轮
                            return;
                        }
                    }
                }

                let _ = self.trigger_snapshot();
            }
        }
    }
}
