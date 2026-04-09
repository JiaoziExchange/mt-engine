use super::OrderBookBackend;
use crate::orders::RestingOrder;
use crate::types::{OrderId, Price, Quantity};
use mt_engine::side::Side;
use rustc_hash::FxHashMap;
use slab::Slab;
use std::collections::{BTreeMap, VecDeque};

/// 价格档位内部结构
struct PriceLevel<OrderIdx> {
    queue: VecDeque<OrderIdx>,
    total_qty: u64,
    price: Price, // 存储价格以支持 O(log N) 移除 [NEW]
}

/// 稀疏下单簿后端 - 基于 BTreeMap 和 Slab 实现
pub struct SparseBackend {
    orders: Slab<RestingOrder<usize>>,
    levels: Slab<PriceLevel<usize>>,
    bids: BTreeMap<Price, usize>,         // Price -> LevelIdx
    asks: BTreeMap<Price, usize>,         // Price -> LevelIdx
    order_map: FxHashMap<OrderId, usize>, // OrderId -> OrderIdx
}

impl Default for SparseBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SparseBackend {
    pub fn new() -> Self {
        Self {
            orders: Slab::new(),
            levels: Slab::new(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            order_map: FxHashMap::default(),
        }
    }
}

impl OrderBookBackend for SparseBackend {
    type OrderIdx = usize;
    type LevelIdx = usize;

    #[inline(always)]
    fn insert_order(&mut self, order: RestingOrder<Self::LevelIdx>) -> Self::OrderIdx {
        let order_id = order.data.order_id;
        let idx = self.orders.insert(order);
        self.order_map.insert(order_id, idx);
        idx
    }

    #[inline(always)]
    fn remove_order(&mut self, order_idx: Self::OrderIdx) -> Option<RestingOrder<Self::LevelIdx>> {
        if self.orders.contains(order_idx) {
            let order = self.orders.remove(order_idx);
            self.order_map.remove(&order.data.order_id);
            Some(order)
        } else {
            None
        }
    }

    #[inline(always)]
    fn get_order(&self, order_idx: Self::OrderIdx) -> Option<&RestingOrder<Self::LevelIdx>> {
        self.orders.get(order_idx)
    }

    #[inline(always)]
    fn get_order_mut(
        &mut self,
        order_idx: Self::OrderIdx,
    ) -> Option<&mut RestingOrder<Self::LevelIdx>> {
        self.orders.get_mut(order_idx)
    }

    #[inline(always)]
    fn get_order_idx_by_id(&self, order_id: OrderId) -> Option<Self::OrderIdx> {
        self.order_map.get(&order_id).copied()
    }

    #[inline(always)]
    fn get_or_create_level(&mut self, side: Side, price: Price) -> Self::LevelIdx {
        let target_map = match side {
            Side::buy => &mut self.bids,
            Side::sell => &mut self.asks,
            _ => panic!("Invalid side"),
        };

        if let Some(&idx) = target_map.get(&price) {
            idx
        } else {
            let idx = self.levels.insert(PriceLevel {
                queue: VecDeque::new(),
                total_qty: 0,
                price, // 存入价格
            });
            target_map.insert(price, idx);
            idx
        }
    }

    #[inline(always)]
    fn get_level(&self, price: Price) -> Option<Self::LevelIdx> {
        self.bids
            .get(&price)
            .or_else(|| self.asks.get(&price))
            .copied()
    }

    #[inline(always)]
    fn best_ask_price(&self) -> Option<Price> {
        self.asks.keys().next().copied()
    }

    #[inline(always)]
    fn best_bid_price(&self) -> Option<Price> {
        self.bids.keys().next_back().copied()
    }

    #[inline(always)]
    fn push_to_level_back(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx) {
        let order_qty = self.orders[order_idx].data.remaining_qty.0;
        if let Some(level) = self.levels.get_mut(level_idx) {
            level.queue.push_back(order_idx);
            level.total_qty += order_qty;
        }
    }

    #[inline(always)]
    fn push_to_level_front(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx) {
        let order_qty = self.orders[order_idx].data.remaining_qty.0;
        if let Some(level) = self.levels.get_mut(level_idx) {
            level.queue.push_front(order_idx);
            level.total_qty += order_qty;
        }
    }

    #[inline(always)]
    fn pop_from_level(&mut self, level_idx: Self::LevelIdx) -> Option<Self::OrderIdx> {
        let level = self.levels.get_mut(level_idx)?;
        if let Some(order_idx) = level.queue.pop_front() {
            let order_qty = self.orders[order_idx].data.remaining_qty.0;
            level.total_qty -= order_qty;
            Some(order_idx)
        } else {
            None
        }
    }

    #[inline(always)]
    fn remove_from_level(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx) {
        if let Some(level) = self.levels.get_mut(level_idx) {
            if let Some(pos) = level.queue.iter().position(|&x| x == order_idx) {
                level.queue.remove(pos);
                // 安全获取数量，防止订单已先行从 orders 中移除
                if let Some(order) = self.orders.get(order_idx) {
                    let order_qty = order.data.remaining_qty.0;
                    level.total_qty = level.total_qty.saturating_sub(order_qty);
                }
            }
        }
    }

    #[inline(always)]
    fn level_order_count(&self, level_idx: Self::LevelIdx) -> usize {
        self.levels
            .get(level_idx)
            .map(|l| l.queue.len())
            .unwrap_or(0)
    }

    #[inline(always)]
    fn remove_empty_level(&mut self, level_idx: Self::LevelIdx) {
        if self.level_order_count(level_idx) == 0 {
            // 利用已存储的 price 直接从红黑树移除，复杂度 O(log N) [FIXED]
            if let Some(level) = self.levels.get(level_idx) {
                let price = level.price;
                self.bids.remove(&price);
                self.asks.remove(&price);
            }
            if self.levels.contains(level_idx) {
                self.levels.remove(level_idx);
            }
        }
    }

    #[inline(always)]
    fn level_total_qty(&self, level_idx: Self::LevelIdx) -> u64 {
        self.levels.get(level_idx).map(|l| l.total_qty).unwrap_or(0)
    }

    #[inline(always)]
    fn get_total_depth(&self, side: Side, price_limit: Price) -> u64 {
        let mut total = 0u64;
        match side {
            Side::buy => {
                for (&_p, &level_idx) in self.bids.range(..=price_limit).rev() {
                    total += self.level_total_qty(level_idx);
                }
            }
            Side::sell => {
                for (&_p, &level_idx) in self.asks.range(price_limit..) {
                    total += self.level_total_qty(level_idx);
                }
            }
            _ => {}
        }
        total
    }

    #[inline(always)]
    fn modify_order_qty(&mut self, order_idx: Self::OrderIdx, new_qty: Quantity) {
        if let Some(order) = self.orders.get_mut(order_idx) {
            let diff = (order.data.remaining_qty.0 as i64) - (new_qty.0 as i64);
            order.data.remaining_qty = new_qty;
            if let Some(level) = self.levels.get_mut(order.level_idx) {
                level.total_qty = (level.total_qty as i64 - diff) as u64;
            }
        }
    }

    #[inline(always)]
    fn prefetch_entry(&self, order_idx: Self::OrderIdx) {
        if let Some(_order) = self.orders.get(order_idx) {
            // 使用自定义宏实现 x86_64 硬件预取
            #[cfg(target_arch = "x86_64")]
            unsafe {
                use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
                _mm_prefetch(&_order.data as *const _ as *const i8, _MM_HINT_T0);
            }
        }
    }

    #[inline(always)]
    fn validate_price(&self, _price: Price) -> bool {
        true
    }
}
