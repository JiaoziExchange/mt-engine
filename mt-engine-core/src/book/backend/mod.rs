use crate::orders::RestingOrder;
#[allow(unused_imports)]
use crate::snapshot::SnapshotModel;
use crate::types::{Price, Quantity};
use mt_engine::side::Side;

pub mod bitset;
pub mod dense;
pub mod sparse;

/// 订单簿后端接口 - 抽象底层存储细节
pub trait OrderBookBackend {
    type OrderIdx: Copy + PartialEq;
    type LevelIdx: Copy + PartialEq;

    // ========== 订单 (Order) 操作 ==========

    /// 插入订单并返回索引
    fn insert_order(&mut self, order: RestingOrder<Self::LevelIdx>) -> Self::OrderIdx;

    /// 移除并返回订单
    fn remove_order(&mut self, order_idx: Self::OrderIdx) -> Option<RestingOrder<Self::LevelIdx>>;

    /// 获取订单只读引用
    fn get_order(&self, order_idx: Self::OrderIdx) -> Option<&RestingOrder<Self::LevelIdx>>;

    /// 获取订单可变引用 (仅用于原位修改数据，如更新数量)
    fn get_order_mut(
        &mut self,
        order_idx: Self::OrderIdx,
    ) -> Option<&mut RestingOrder<Self::LevelIdx>>;

    /// 通过 OrderId 快速定位订单索引
    fn get_order_idx_by_id(&self, order_id: crate::types::OrderId) -> Option<Self::OrderIdx>;

    // ========== 档位 (Level) 操作 ==========

    /// 获取或创建价格档位
    fn get_or_create_level(&mut self, side: Side, price: Price) -> Self::LevelIdx;

    /// 获取已有档位 (不创建)
    fn get_level(&self, price: Price) -> Option<Self::LevelIdx>;

    /// 最优卖单价 (Lowest Ask)
    fn best_ask_price(&self) -> Option<Price>;

    /// 最优买单价 (Highest Bid)
    fn best_bid_price(&self) -> Option<Price>;

    // ========== 队列 (Queue) 操作 ==========

    /// 将订单推入指定级别的队列末尾 (用于新挂单)
    fn push_to_level_back(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx);

    /// 将订单推入指定级别的队列头部 (用于部分成交后的 Maker 回插)
    fn push_to_level_front(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx);

    /// 从指定级别的队列头部弹出订单 (用于撮合)
    fn pop_from_level(&mut self, level_idx: Self::LevelIdx) -> Option<Self::OrderIdx>;

    /// 从队列中移除特定订单 (用于撤单/改单)
    fn remove_from_level(&mut self, level_idx: Self::LevelIdx, order_idx: Self::OrderIdx);

    /// 获取档位下的订单数量
    fn level_order_count(&self, level_idx: Self::LevelIdx) -> usize;

    /// 移除空档位 (如果该档位已无订单)
    fn remove_empty_level(&mut self, level_idx: Self::LevelIdx);

    /// 获取指定级别的聚合数量 [NEW]
    fn level_total_qty(&self, level_idx: Self::LevelIdx) -> u64;

    /// 获取累积深度 (从最优价到指定价格) [NEW]
    fn get_total_depth(&self, side: Side, price_limit: Price) -> u64;

    /// 修正订单数量并同步级别聚合数量 (用于原位更新) [NEW]
    fn modify_order_qty(&mut self, order_idx: Self::OrderIdx, new_qty: Quantity);

    /// 预取指令支持 - 提前加载订单数据到 L1 缓存 [NEW]
    fn prefetch_entry(&self, order_idx: Self::OrderIdx);

    /// 验证价格是否在合法范围内 (用于某些有价格范围限制的后端) [NEW]
    fn validate_price(&self, price: Price) -> bool;

    // ========== 快照 (Snapshot) 支持 ==========

    /// 将后端状态转换为中立的快照模型中的档位数据
    fn export_levels(&self) -> Vec<crate::snapshot::PriceLevelModel> {
        unimplemented!("Snapshot exporting is not supported by this backend")
    }

    /// 从中立的快照模型中的档位数据恢复后端状态
    fn import_levels(&mut self, levels: Vec<crate::snapshot::PriceLevelModel>);
}
