use super::bitset::L3Bitset;
use super::OrderBookBackend;
use crate::orders::RestingOrder;
use crate::types::{OrderId, Price, Quantity};
use mt_engine::side::Side;

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
    pub id_to_index: Vec<u32>,
}

impl DenseBackend {
    pub fn new(config: PriceRange, capacity: usize) -> Self {
        assert!(config.tick.0 > 0, "Tick size must be greater than 0");
        let depth = ((config.max.0 - config.min.0) / config.tick.0) as usize + 1;

        let mut free_list = Vec::with_capacity(capacity);
        for i in (1..=capacity as u32).rev() {
            free_list.push(i);
        }

        let id_to_index = vec![0; capacity + 1];

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
            id_to_index,
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
    fn insert_order(
        &mut self,
        order: RestingOrder<Self::LevelIdx>,
    ) -> Result<Self::OrderIdx, crate::outcome::CommandFailure> {
        let order_id = order.data.order_id.0 as usize;

        // 1. Safety boundary check: prevent OOM or out-of-bounds caused by maliciously large IDs
        if order_id >= self.id_to_index.len() {
            return Err(crate::outcome::CommandFailure::InvalidOrderId);
        }

        // 2. Idempotency/Duplicate check: O(1) verification based on physical index
        if self.id_to_index[order_id] != 0 {
            return Err(crate::outcome::CommandFailure::DuplicateOrderId);
        }

        // 3. Order pool water level check: gracefully handle load saturation
        let idx = match self.free_list.pop() {
            Some(idx) => idx,
            None => return Err(crate::outcome::CommandFailure::CapacityExceeded),
        };

        self.order_pool[idx as usize] = order;
        self.order_links[idx as usize] = OrderLink {
            prev: 0,
            next: 0,
            level_idx: order.level_idx,
            _padding: 0,
        };
        self.id_to_index[order_id] = idx;
        Ok(idx)
    }

    #[inline(always)]
    fn remove_order(&mut self, order_idx: Self::OrderIdx) -> Option<RestingOrder<Self::LevelIdx>> {
        if order_idx == 0 {
            return None;
        }
        let order = self.order_pool[order_idx as usize];
        let order_id = order.data.order_id.0 as usize;

        // Clear physical index mapping
        if order_id < self.id_to_index.len() {
            self.id_to_index[order_id] = 0;
        }

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
        let id = order_id.0 as usize;
        if id < self.id_to_index.len() {
            let idx = self.id_to_index[id];
            if idx != 0 {
                return Some(idx);
            }
        }
        None
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
        // Sync visible quantity to prevent "Ghost Peaks" (visible_qty > remaining_qty)
        order.data.visible_qty.0 = std::cmp::min(order.data.visible_qty.0, new_qty.0);

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
            // Prefetch order data
            _mm_prefetch(
                &self.order_pool[order_idx as usize].data as *const _ as *const i8,
                _MM_HINT_T0,
            );

            // Optional: Prefetch next link
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
                // Optimization: only scan from the highest bid down to the limit
                if let Some(best_idx) = self.bids_bitset.find_last(self.depth) {
                    if best_idx >= limit_idx {
                        for idx in (limit_idx..=best_idx).rev() {
                            if let Some(level) = &self.level_array[idx] {
                                total += level.total_qty;
                            }
                        }
                    }
                }
            }
            Side::sell => {
                let limit_idx = self.price_to_idx(price_limit).unwrap_or(self.depth - 1);
                // Optimization: only scan from the lowest ask up to the limit
                if let Some(best_idx) = self.asks_bitset.find_first(self.depth) {
                    if best_idx <= limit_idx {
                        for idx in best_idx..=limit_idx {
                            if let Some(level) = &self.level_array[idx] {
                                total += level.total_qty;
                            }
                        }
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

    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    fn export_levels(&self) -> Vec<crate::snapshot::PriceLevelModel> {
        let mut model_levels = Vec::new();
        for idx in 0..self.depth {
            if let Some(level) = &self.level_array[idx] {
                let price = self.idx_to_price(idx);
                let side = if self.bids_bitset.test(idx) {
                    Side::buy
                } else {
                    Side::sell
                };

                let mut orders = Vec::with_capacity(level.count as usize);
                let mut curr = level.head;
                while curr != 0 {
                    orders.push(self.order_pool[curr as usize].data);
                    curr = self.order_links[curr as usize].next;
                }
                model_levels.push(crate::snapshot::PriceLevelModel {
                    price,
                    side,
                    orders,
                });
            }
        }
        model_levels
    }

    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    fn import_levels(&mut self, levels: Vec<crate::snapshot::PriceLevelModel>) {
        // Clear existing state
        self.bids_bitset.clear();
        self.asks_bitset.clear();
        self.level_array.fill(None);
        self.id_to_index.fill(0);
        self.free_list.clear();
        let capacity = self.order_pool.len() - 1;
        for i in (1..=capacity as u32).rev() {
            self.free_list.push(i);
        }

        for model_level in levels {
            let level_idx = self.get_or_create_level(model_level.side, model_level.price);
            for order_data in model_level.orders {
                let order_idx = self
                    .insert_order(RestingOrder::new(order_data, level_idx))
                    .unwrap();
                self.push_to_level_back(level_idx, order_idx);
            }
        }
    }

    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    fn transfer_to_sparse(&self) -> crate::book::backend::sparse::SparseBackend {
        let mut sparse = crate::book::backend::sparse::SparseBackend::new();
        sparse.import_levels(self.export_levels());
        sparse
    }

    #[cfg(feature = "rkyv")]
    fn import_from_archived_sparse(
        &mut self,
        archived: &rkyv::Archived<crate::book::backend::sparse::SparseBackend>,
    ) {
        self.adapt_from_archived_sparse(archived);
    }
}

impl DenseBackend {
    /// Adapter logic: Restore state from archived SparseBackend format to DenseBackend (shadow type isolation)
    #[cfg(feature = "rkyv")]
    fn adapt_from_archived_sparse(
        &mut self,
        archived: &crate::book::backend::sparse::ArchivedSparseBackend,
    ) {
        // Clear existing state
        self.bids_bitset.clear();
        self.asks_bitset.clear();
        self.level_array.fill(None);
        self.id_to_index.fill(0);
        self.free_list.clear();
        for i in (1..self.order_pool.len() as u32).rev() {
            self.free_list.push(i);
        }

        use crate::orders::{OrderData, RestingOrder};
        use crate::types::Price;
        use mt_engine::side::Side;
        use rkyv::{Archived, Deserialize};

        // Process bid and ask orders (iterate via ArchivedBTreeMap)
        // Note: On the current platform, Archived<usize> is u32, and Archived<Price> is ArchivedPrice
        let process_map =
            |this: &mut Self,
             map: &rkyv::collections::ArchivedBTreeMap<Archived<Price>, Archived<usize>>,
             side: Side| {
                for (archived_price, archived_level_idx) in map.iter() {
                    let price = Price(archived_price.0);
                    if !this.validate_price(price) {
                        continue;
                    }

                    let level_idx = this.get_or_create_level(side, price);

                    // Get price level information from the levels Slab of ArchivedSparseBackend
                    let sparse_level_idx = u64::from(*archived_level_idx) as usize;

                    if let Some(archived_level_opt) = archived.levels.get(sparse_level_idx) {
                        if let Some(archived_level) = archived_level_opt.as_ref() {
                            // Iterate through the order queue for this price level
                            for archived_sparse_order_idx in archived_level.queue.iter() {
                                let order_idx = u64::from(*archived_sparse_order_idx) as usize;
                                if let Some(archived_order_opt) = archived.orders.get(order_idx) {
                                    if let Some(archived_order) = archived_order_opt.as_ref() {
                                        let order_data: OrderData = archived_order
                                            .data
                                            .deserialize(&mut rkyv::Infallible)
                                            .unwrap();

                                        let dense_order_idx = this
                                            .insert_order(RestingOrder::new(order_data, level_idx))
                                            .unwrap();
                                        this.push_to_level_back(level_idx, dense_order_idx);
                                    }
                                }
                            }
                        }
                    }
                }
            };

        process_map(self, &archived.bids, Side::buy);
        process_map(self, &archived.asks, Side::sell);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Price;

    #[test]
    fn test_dense_price_level() {
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

        let idx = backend.insert_order(RestingOrder::new(data, 50)).unwrap();
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
        data1.order_id = OrderId(1);
        data1.remaining_qty = Quantity(10);
        let id1 = backend
            .insert_order(RestingOrder::new(data1, level))
            .unwrap();

        let mut data2: crate::orders::OrderData = unsafe { std::mem::zeroed() };
        data2.order_id = OrderId(2);
        data2.remaining_qty = Quantity(20);
        let id2 = backend
            .insert_order(RestingOrder::new(data2, level))
            .unwrap();

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
