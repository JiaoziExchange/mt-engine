use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::order_flags::OrderFlags;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// 订单基础数据
///
/// 按照撮合引擎热路径访问频率排列字段：
/// 1. 剩余数量 / 已成交数量 / 价格 (撮合必调)
/// 2. 可见数量 / 峰值大小 (冰山单逻辑)
/// 3. 方向 / 标志位 / 类型 (逻辑分支)
#[repr(C, align(128))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OrderData {
    // ========== 热数据 (Hot Data - First Cache Line) ==========
    /// 剩余数量 (Total Remaining)
    pub remaining_qty: Quantity,
    /// 已成交数量 (Total Filled)
    pub filled_qty: Quantity,
    /// 价格 (Limit Price)
    pub price: Price,
    /// 触发价格 (Stop/StopLimit)
    pub trigger_price: Price,
    /// 有效期截止时间戳 (GTD/GTH)
    pub expiry: Timestamp,
    /// 冰山单可见数量 (Current Visible Peak)
    pub visible_qty: Quantity,
    /// 冰山单峰值大小 (Original Peak Size)
    pub peak_size: Quantity,
    /// 订单类型 (Market, Limit, Stop, etc.)
    pub order_type: OrderType,
    /// 买卖方向
    pub side: Side,
    /// 订单状态标志 (Post-Only, Iceberg, etc.)
    pub flags: OrderFlags,

    // ========== 冷数据 (Cold Data - Second Cache Line) ==========
    /// 订单唯一标识
    pub order_id: OrderId,
    /// 用户 ID
    pub user_id: UserId,
    /// 序列号
    pub sequence_number: SequenceNumber,
    /// 提交时间戳
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

/// 订单簿中的挂单 (Resting Order)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RestingOrder<L> {
    pub data: OrderData,
    pub level_idx: L,
}

impl<L> RestingOrder<L> {
    pub fn new(data: OrderData, level_idx: L) -> Self {
        Self { data, level_idx }
    }
}
