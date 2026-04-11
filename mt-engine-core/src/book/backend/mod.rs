use crate::orders::RestingOrder;
use crate::types::{Price, Quantity};
use mt_engine::side::Side;

pub mod bitset;
pub mod dense;
pub mod sparse;

/// Order Book Backend Interface - abstracts underlying storage details
pub trait OrderBookBackend {
    type OrderIdx: Copy + PartialEq;
    type LevelIdx: Copy + PartialEq;

    // ========== Order Operations ==========

    /// Insert an order and return its index; return an error if resources are insufficient or ID conflicts occur
    fn insert_order(
        &mut self,
        order: RestingOrder<Self::LevelIdx>,
    ) -> Result<Self::OrderIdx, crate::outcome::CommandFailure>;

    /// Remove and return the order
    fn remove_order(&mut self, order_idx: Self::OrderIdx) -> Option<RestingOrder<Self::LevelIdx>>;

    /// Get read-only reference to an order
    fn get_order(&self, order_idx: Self::OrderIdx) -> Option<&RestingOrder<Self::LevelIdx>>;

    /// Get mutable reference to an order (only for in-place modifications, e.g., updating quantity)
    fn get_order_mut(
        &mut self,
        order_idx: Self::OrderIdx,
    ) -> Option<&mut RestingOrder<Self::LevelIdx>>;

    /// Fast lookup of order index by OrderId
    fn get_order_idx_by_id(&self, order_id: crate::types::OrderId) -> Option<Self::OrderIdx>;

    // ========== Price Level Operations ==========

    /// Get or create a price level
    fn get_or_create_level(&mut self, side: Side, price: Price) -> Self::LevelIdx;

    /// Get existing level (does not create)
    fn get_level(&self, price: Price) -> Option<Self::LevelIdx>;

    /// Lowest Ask Price
    fn best_ask_price(&self) -> Option<Price>;

    /// Highest Bid Price
    fn best_bid_price(&self) -> Option<Price>;

    // ========== Queue Operations ==========

    /// Push order to the end of the specified level's queue (for new limit orders)
    fn push_to_level_back(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx);

    /// Push order to the front of the specified level's queue (for maker re-insertion after partial fill)
    fn push_to_level_front(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx);

    /// Pop order from the front of the specified level's queue (for matching)
    fn pop_from_level(&mut self, level_idx: Self::LevelIdx) -> Option<Self::OrderIdx>;

    /// Remove specific order from the queue (for cancellation/amendment)
    fn remove_from_level(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx);

    /// Get number of orders at a price level
    fn level_order_count(&self, level_idx: Self::LevelIdx) -> usize;

    /// Remove empty level (if no orders remaining)
    fn remove_empty_level(&mut self, level_idx: Self::LevelIdx);

    /// Get aggregate quantity at a price level [NEW]
    fn level_total_qty(&self, level_idx: Self::LevelIdx) -> u64;

    /// Get cumulative depth (from best price to limit price) [NEW]
    fn get_total_depth(&self, side: Side, price_limit: Price) -> u64;

    /// Modify order quantity and sync level aggregate quantity (for in-place updates) [NEW]
    fn modify_order_qty(&mut self, order_idx: Self::OrderIdx, new_qty: Quantity);

    /// Prefetch instruction support - load order data into L1 cache in advance [NEW]
    fn prefetch_entry(&self, order_idx: Self::OrderIdx);

    /// Validate if price is within legal range (for backends with price range limits) [NEW]
    fn validate_price(&self, price: Price) -> bool;

    // ========== Snapshot Support ==========

    /// Convert backend state to neutral price level data in the snapshot model
    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    fn export_levels(&self) -> Vec<crate::snapshot::PriceLevelModel> {
        unimplemented!("Snapshot exporting is not supported by this backend")
    }

    /// Restore backend state from neutral price level data in the snapshot model
    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    fn import_levels(&mut self, levels: Vec<crate::snapshot::PriceLevelModel>);

    /// [NEW] Convert backend state to neutral SparseBackend model (for snapshot export)
    #[cfg(any(feature = "snapshot", feature = "serde", feature = "rkyv"))]
    fn transfer_to_sparse(&self) -> crate::book::backend::sparse::SparseBackend;

    /// [NEW] Restore state from archived SparseBackend shadow model (shadow type isolation)
    #[cfg(feature = "rkyv")]
    fn import_from_archived_sparse(
        &mut self,
        archived: &rkyv::Archived<crate::book::backend::sparse::SparseBackend>,
    );
}
