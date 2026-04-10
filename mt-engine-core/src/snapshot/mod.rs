use crate::orders::OrderData;
use crate::types::{OrderId, Price, SequenceNumber, Timestamp};
use mt_engine::side::Side;
use rkyv::{Archive, Deserialize, Serialize};

#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

pub mod rkyv_util;

/// 快照模型 - 包含引擎完整状态的 rkyv 版本
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct SnapshotModel {
    pub last_sequence_number: SequenceNumber,
    pub last_timestamp: Timestamp,
    pub trade_id_seq: u64,
    pub ltp: Price,
    pub last_order_id: OrderId,

    /// 核心后端状态 (直接归档 Sparse 结构)
    pub backend: crate::book::backend::sparse::SparseBackend,

    /// 条件单状态 (使用 SlabWrapper)
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

/// PriceLevelModel - 档位快照模型
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct PriceLevelModel {
    pub price: Price,
    pub side: Side,
    pub orders: Vec<OrderData>,
}

/// 快照配置
#[cfg(feature = "snapshot")]
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct SnapshotConfig {
    /// 指令计数间隔（每 N 条指令触发一次）
    pub count_interval: u64,
    /// 时间间隔（基于业务时间戳，毫秒）
    pub time_interval_ms: u64,
    /// 快照文件存储路径模板
    pub path_template: String,
    /// 是否开启压缩 (默认开启)
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
