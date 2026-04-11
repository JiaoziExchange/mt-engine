use crate::orders::OrderData;
use crate::types::{OrderId, Price, SequenceNumber, Timestamp};
use mt_engine::side::Side;
use rkyv::{Archive, Deserialize, Serialize};

#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

pub mod rkyv_util;

/// Snapshot Model - rkyv version containing the full engine state
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct SnapshotModel {
    pub last_sequence_number: SequenceNumber,
    pub last_timestamp: Timestamp,
    pub trade_id_seq: u64,
    pub ltp: Price,
    pub last_order_id: OrderId,

    /// Core backend state (directly archives the Sparse structure)
    pub backend: crate::book::backend::sparse::SparseBackend,

    /// Conditional order state (using SlabWrapper)
    #[with(crate::snapshot::rkyv_util::SlabWrapper)]
    pub condition_orders: slab::Slab<OrderData>,
}

impl Default for SnapshotModel {
    fn default() -> Self {
        Self {
            last_sequence_number: SequenceNumber(0),
            last_timestamp: Timestamp(0),
            trade_id_seq: 0,
            ltp: Price(0),
            last_order_id: OrderId(0),
            backend: crate::book::backend::sparse::SparseBackend::new(),
            condition_orders: slab::Slab::new(),
        }
    }
}

/// PriceLevelModel - Price level snapshot model
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct PriceLevelModel {
    pub price: Price,
    pub side: Side,
    pub orders: Vec<OrderData>,
}

/// Snapshot Configuration
#[cfg(feature = "snapshot")]
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct SnapshotConfig {
    /// Command count interval (triggered every N commands)
    pub count_interval: u64,
    /// Time interval (based on business timestamp, in milliseconds)
    pub time_interval_ms: u64,
    /// Snapshot file storage path template
    pub path_template: String,
    /// Whether to enable compression (enabled by default)
    pub compress: bool,
}

#[cfg(feature = "snapshot")]
impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            count_interval: 1_000_000,
            time_interval_ms: 600_000, // 10 minutes
            path_template: "./snapshots/snapshot_{seq}.bin.zst".to_string(),
            compress: true,
        }
    }
}
