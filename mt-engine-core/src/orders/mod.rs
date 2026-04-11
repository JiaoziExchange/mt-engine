use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::order_flags::OrderFlags;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use rkyv::{Archive, Deserialize, Serialize};

#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Base order data
///
/// Fields ordered by access frequency in the matching engine hot path:
/// 1. Remaining Qty / Filled Qty / Price (required for matching)
/// 2. Visible Qty / Peak Size (Iceberg order logic)
/// 3. Side / Flags / Order Type (logic branches)
#[repr(C, align(128))]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct OrderData {
    // ========== Hot Data (First Cache Line) ==========
    /// Total remaining quantity
    pub remaining_qty: Quantity,
    /// Total filled quantity
    pub filled_qty: Quantity,
    /// Limit price
    pub price: Price,
    /// Trigger price (Stop/StopLimit)
    pub trigger_price: Price,
    /// Expiry timestamp (GTD/GTH)
    pub expiry: Timestamp,
    /// Iceberg visible quantity (Current Visible Peak)
    pub visible_qty: Quantity,
    /// Iceberg peak size (Original Peak Size)
    pub peak_size: Quantity,
    /// Order type (Market, Limit, Stop, etc.)
    pub order_type: OrderType,
    /// Buy/Sell side
    pub side: Side,
    /// Order status flags (Post-Only, Iceberg, etc.)
    pub flags: OrderFlags,

    // ========== Cold Data (Second Cache Line) ==========
    /// Order unique identifier
    pub order_id: OrderId,
    /// User ID
    pub user_id: UserId,
    /// Sequence number
    pub sequence_number: SequenceNumber,
    /// Submission timestamp
    pub timestamp: Timestamp,
}

impl OrderData {
    #[inline]
    pub fn is_fully_filled(&self) -> bool {
        self.remaining_qty.0 == 0
    }

    #[inline]
    pub fn is_iceberg(&self) -> bool {
        self.flags.get_iceberg() && self.peak_size.0 > 0
    }

    #[inline]
    pub fn is_stop(&self) -> bool {
        matches!(self.order_type, OrderType::stop | OrderType::stop_limit)
    }

    #[inline]
    pub fn is_expired(&self, current_ts: Timestamp) -> bool {
        self.expiry.0 > 0 && current_ts.0 >= self.expiry.0
    }
}

/// Resting Order in the order book
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct RestingOrder<L> {
    pub data: OrderData,
    pub level_idx: L,
}

impl<L> RestingOrder<L> {
    pub fn new(data: OrderData, level_idx: L) -> Self {
        Self { data, level_idx }
    }
}
