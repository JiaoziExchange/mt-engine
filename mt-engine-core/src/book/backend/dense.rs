use super::bitset::L3Bitset;
use super::OrderBookBackend;
use crate::orders::RestingOrder;
use crate::types::{OrderId, Price, Quantity};
use mt_engine::side::Side;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug)]
pub struct PriceRange {
    pub min: Price,
    pub max: Price,
    pub tick: Price,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OrderLink {
    pub prev: u32,
    pub next: u32,
    pub level_idx: u32,
    pub _padding: u32,
}

#[derive(Clone, Copy, Default)]
pub struct LevelData {
    pub head: u32,
    pub tail: u32,
    pub count: u32,
    pub total_qty: u64,
}

pub struct DenseBackend {
    pub config: PriceRange,
    pub depth: usize,
    pub bids_bitset: L3Bitset,
    pub asks_bitset: L3Bitset,
    pub level_array: Vec<Option<LevelData>>,
    pub order_pool: Vec<RestingOrder<u32>>,
    pub order_links: Vec<OrderLink>,
    pub free_list: Vec<u32>,
    pub order_map: FxHashMap<OrderId, u32>,
}

impl DenseBackend {
    pub fn new(config: PriceRange, capacity: usize) -> Self {
        assert!(config.tick.0 > 0, "Tick size must be greater than 0"); // 边界溢出保护：防止除零异常
        let depth = ((config.max.0 - config.min.0) / config.tick.0) as usize + 1;

        let mut free_list = Vec::with_capacity(capacity);
        for i in (1..=capacity as u32).rev() {
            // 0 被保留为无效指针 (null pointer)
            free_list.push(i);
        }

        // 初始化池子为默认值，长度为 capacity + 1（以 1 为基数的索引）
        // 索引 0 作为虚拟订单占位。使用 MaybeUninit 更稳健以应对未来的字段扩展
        let dummy_order = RestingOrder {
            data: unsafe { std::mem::MaybeUninit::zeroed().assume_init() },
            level_idx: 0,
        };

        Self {
            config,
            depth,
            bids_bitset: L3Bitset::new(depth),
            asks_bitset: L3Bitset::new(depth),
            level_array: vec![None; depth],
            order_pool: vec![dummy_order; capacity + 1],
            order_links: vec![OrderLink::default(); capacity + 1],
            free_list,
            order_map: FxHashMap::default(),
        }
    }

    #[inline(always)]
    pub fn price_to_idx(&self, price: Price) -> Option<usize> {
        if price < self.config.min || price > self.config.max {
            return None;
        }
        Some(((price.0 - self.config.min.0) / self.config.tick.0) as usize)
    }

    #[inline(always)]
    pub fn idx_to_price(&self, idx: usize) -> Price {
        Price(self.config.min.0 + (idx as u64 * self.config.tick.0))
    }
}

impl OrderBookBackend for DenseBackend {
    type OrderIdx = u32;
    type LevelIdx = u32;

    #[inline(always)]
    fn get_or_create_level(&mut self, side: Side, price: Price) -> Self::LevelIdx {
        let idx = self.price_to_idx(price).expect("Price out of bounds");
        if self.level_array[idx].is_none() {
            self.level_array[idx] = Some(LevelData::default());
            match side {
                Side::buy => self.bids_bitset.set(idx),
                Side::sell => self.asks_bitset.set(idx),
                _ => {}
            }
        }
        idx as u32
    }

    #[inline(always)]
    fn get_level(&self, price: Price) -> Option<Self::LevelIdx> {
        let idx = self.price_to_idx(price)?;
        if self.level_array[idx].is_some() {
            Some(idx as u32)
        } else {
            None
        }
    }

    #[inline(always)]
    fn best_bid_price(&self) -> Option<Price> {
        self.bids_bitset
            .find_last(self.depth)
            .map(|idx| self.idx_to_price(idx))
    }

    #[inline(always)]
    fn best_ask_price(&self) -> Option<Price> {
        self.asks_bitset
            .find_first(self.depth)
            .map(|idx| self.idx_to_price(idx))
    }

    #[inline(always)]
    fn remove_empty_level(&mut self, level_idx: Self::LevelIdx) {
        let idx = level_idx as usize;
        if let Some(level) = &self.level_array[idx] {
            if level.count == 0 {
                self.level_array[idx] = None;
                self.bids_bitset.unset(idx);
                self.asks_bitset.unset(idx);
            }
        }
    }

    #[inline(always)]
    fn insert_order(&mut self, order: RestingOrder<Self::LevelIdx>) -> Self::OrderIdx {
        let idx = self.free_list.pop().expect("Order pool exhausted");
        self.order_pool[idx as usize] = order;
        self.order_links[idx as usize] = OrderLink {
            prev: 0,
            next: 0,
            level_idx: order.level_idx,
            _padding: 0,
        };
        self.order_map.insert(order.data.order_id, idx);
        idx
    }

    #[inline(always)]
    fn remove_order(&mut self, order_idx: Self::OrderIdx) -> Option<RestingOrder<Self::LevelIdx>> {
        if order_idx == 0 {
            return None;
        }
        let order = self.order_pool[order_idx as usize];
        self.order_map.remove(&order.data.order_id);
        self.free_list.push(order_idx);
        Some(order)
    }

    #[inline(always)]
    fn get_order(&self, order_idx: Self::OrderIdx) -> Option<&RestingOrder<Self::LevelIdx>> {
        if order_idx == 0 {
            None
        } else {
            Some(&self.order_pool[order_idx as usize])
        }
    }

    #[inline(always)]
    fn get_order_mut(
        &mut self,
        order_idx: Self::OrderIdx,
    ) -> Option<&mut RestingOrder<Self::LevelIdx>> {
        if order_idx == 0 {
            None
        } else {
            Some(&mut self.order_pool[order_idx as usize])
        }
    }

    #[inline(always)]
    fn get_order_idx_by_id(&self, order_id: OrderId) -> Option<Self::OrderIdx> {
        self.order_map.get(&order_id).copied()
    }
    #[inline(always)]
    fn push_to_level_back(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx) {
        if let Some(level) = &mut self.level_array[level_idx as usize] {
            let order_qty = self.order_pool[order_idx as usize].data.remaining_qty.0;
            if level.tail == 0 {
                level.head = order_idx;
                level.tail = order_idx;
            } else {
                self.order_links[level.tail as usize].next = order_idx;
                self.order_links[order_idx as usize].prev = level.tail;
                self.order_links[order_idx as usize].next = 0;
                level.tail = order_idx;
            }
            level.count += 1;
            level.total_qty += order_qty;
        }
    }

    #[inline(always)]
    fn push_to_level_front(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx) {
        if let Some(level) = &mut self.level_array[level_idx as usize] {
            let order_qty = self.order_pool[order_idx as usize].data.remaining_qty.0;
            if level.head == 0 {
                level.head = order_idx;
                level.tail = order_idx;
            } else {
                self.order_links[level.head as usize].prev = order_idx;
                self.order_links[order_idx as usize].next = level.head;
                self.order_links[order_idx as usize].prev = 0;
                level.head = order_idx;
            }
            level.count += 1;
            level.total_qty += order_qty;
        }
    }

    #[inline(always)]
    fn pop_from_level(&mut self, level_idx: Self::LevelIdx) -> Option<Self::OrderIdx> {
        let level = self.level_array[level_idx as usize].as_mut()?;
        if level.head == 0 {
            return None;
        }

        let order_idx = level.head;
        let order_qty = self.order_pool[order_idx as usize].data.remaining_qty.0;

        let next_idx = self.order_links[order_idx as usize].next;
        if next_idx == 0 {
            level.head = 0;
            level.tail = 0;
        } else {
            self.order_links[next_idx as usize].prev = 0;
            level.head = next_idx;
        }

        self.order_links[order_idx as usize].prev = 0;
        self.order_links[order_idx as usize].next = 0;

        level.count -= 1;
        level.total_qty = level.total_qty.saturating_sub(order_qty);
        Some(order_idx)
    }

    #[inline(always)]
    fn remove_from_level(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx) {
        let level = match self.level_array[level_idx as usize].as_mut() {
            Some(l) => l,
            None => return,
        };

        let link = self.order_links[order_idx as usize];
        let order_qty = self.order_pool[order_idx as usize].data.remaining_qty.0;

        if link.prev == 0 {
            level.head = link.next;
        } else {
            self.order_links[link.prev as usize].next = link.next;
        }

        if link.next == 0 {
            level.tail = link.prev;
        } else {
            self.order_links[link.next as usize].prev = link.prev;
        }

        self.order_links[order_idx as usize].prev = 0;
        self.order_links[order_idx as usize].next = 0;

        level.count -= 1;
        level.total_qty = level.total_qty.saturating_sub(order_qty);
    }

    #[inline(always)]
    fn level_order_count(&self, level_idx: Self::LevelIdx) -> usize {
        self.level_array[level_idx as usize]
            .map(|l| l.count as usize)
            .unwrap_or(0)
    }

    #[inline(always)]
    fn level_total_qty(&self, level_idx: Self::LevelIdx) -> u64 {
        self.level_array[level_idx as usize]
            .map(|l| l.total_qty)
            .unwrap_or(0)
    }

    #[inline(always)]
    fn modify_order_qty(&mut self, order_idx: Self::OrderIdx, new_qty: Quantity) {
        if order_idx == 0 {
            return;
        }
        let order = &mut self.order_pool[order_idx as usize];
        let diff = (order.data.remaining_qty.0 as i64) - (new_qty.0 as i64);
        order.data.remaining_qty = new_qty;

        let level_idx = order.level_idx;
        if let Some(level) = &mut self.level_array[level_idx as usize] {
            level.total_qty = (level.total_qty as i64 - diff) as u64;
        }
    }

    #[inline(always)]
    #[allow(clippy::needless_return)]
    fn prefetch_entry(&self, order_idx: Self::OrderIdx) {
        if order_idx == 0 {
            return;
        }
        #[cfg(target_arch = "x86_64")]
        unsafe {
            use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
            // 预取订单数据
            _mm_prefetch(
                &self.order_pool[order_idx as usize].data as *const _ as *const i8,
                _MM_HINT_T0,
            );

            // 可选：预取下一条链接
            let link = &self.order_links[order_idx as usize];
            if link.next != 0 {
                _mm_prefetch(
                    &self.order_pool[link.next as usize].data as *const _ as *const i8,
                    _MM_HINT_T0,
                );
            }
        }
    }

    #[inline(always)]
    fn get_total_depth(&self, side: Side, price_limit: Price) -> u64 {
        let mut total = 0u64;
        match side {
            Side::buy => {
                let limit_idx = self.price_to_idx(price_limit).unwrap_or(0);
                // 遍历买单：向后（从高到限价）
                for idx in (limit_idx..self.depth).rev() {
                    if let Some(level) = &self.level_array[idx] {
                        total += level.total_qty;
                    }
                }
            }
            Side::sell => {
                let limit_idx = self.price_to_idx(price_limit).unwrap_or(self.depth - 1);
                // 遍历卖单：向前（从低到限价）
                for idx in 0..=limit_idx {
                    if let Some(level) = &self.level_array[idx] {
                        total += level.total_qty;
                    }
                }
            }
            _ => {}
        }
        total
    }

    #[inline(always)]
    fn validate_price(&self, price: Price) -> bool {
        price >= self.config.min && price <= self.config.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Price;

    #[test]
    fn test_dense_backend_init() {
        let config = PriceRange {
            min: Price(100),
            max: Price(200),
            tick: Price(1),
        };
        let backend = DenseBackend::new(config, 1024);
        assert_eq!(backend.price_to_idx(Price(150)), Some(50));
    }

    #[test]
    fn test_dense_level_management() {
        let config = PriceRange {
            min: Price(100),
            max: Price(200),
            tick: Price(1),
        };
        let mut backend = DenseBackend::new(config, 1024);

        let level1 = backend.get_or_create_level(Side::buy, Price(150));
        let _level2 = backend.get_or_create_level(Side::sell, Price(160));

        assert_eq!(backend.best_bid_price(), Some(Price(150)));
        assert_eq!(backend.best_ask_price(), Some(Price(160)));

        backend.remove_empty_level(level1);
        assert_eq!(backend.best_bid_price(), None);
    }

    #[test]
    fn test_dense_order_allocation() {
        let config = PriceRange {
            min: Price(100),
            max: Price(200),
            tick: Price(1),
        };
        let mut backend = DenseBackend::new(config, 1024);

        let mut data: crate::orders::OrderData = unsafe { std::mem::zeroed() };
        data.order_id = OrderId(42);

        let idx = backend.insert_order(RestingOrder::new(data, 50));
        assert!(idx > 0);
        assert_eq!(backend.get_order_idx_by_id(OrderId(42)), Some(idx));

        let removed = backend.remove_order(idx);
        assert!(removed.is_some());
        assert_eq!(backend.get_order_idx_by_id(OrderId(42)), None);
    }

    #[test]
    fn test_dense_intrusive_list() {
        let config = PriceRange {
            min: Price(100),
            max: Price(200),
            tick: Price(1),
        };
        let mut backend = DenseBackend::new(config, 1024);

        let level = backend.get_or_create_level(Side::buy, Price(150));

        let mut data1: crate::orders::OrderData = unsafe { std::mem::zeroed() };
        data1.remaining_qty = Quantity(10);
        let id1 = backend.insert_order(RestingOrder::new(data1, level));

        let mut data2: crate::orders::OrderData = unsafe { std::mem::zeroed() };
        data2.remaining_qty = Quantity(20);
        let id2 = backend.insert_order(RestingOrder::new(data2, level));

        backend.push_to_level_back(level, id1);
        backend.push_to_level_back(level, id2);

        assert_eq!(backend.level_order_count(level), 2);
        assert_eq!(backend.level_total_qty(level), 30);

        assert_eq!(backend.pop_from_level(level), Some(id1));
        assert_eq!(backend.level_order_count(level), 1);

        backend.remove_from_level(level, id2);
        assert_eq!(backend.level_order_count(level), 0);
    }
}
