use crate::orders::OrderData;
use crate::types::{Price, SequenceNumber, Timestamp};
use mt_engine::side::Side;
use serde::{Deserialize, Serialize};

/// 快照中间模型 - 后端无关的订单簿状态表示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotModel {
    pub last_sequence_number: SequenceNumber,
    pub last_timestamp: Timestamp,
    pub trade_id_seq: u64,
    pub ltp: Price,
    pub levels: Vec<PriceLevelModel>,
    pub condition_orders: Vec<OrderData>,
}

/// 价格档位中间模型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelModel {
    pub price: Price,
    pub side: Side,
    pub orders: Vec<OrderData>,
}

/// 快照配置
#[cfg(feature = "snapshot")]
#[derive(Debug, Clone, Serialize, Deserialize)]
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
