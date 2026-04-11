use crate::orders::OrderData;
use crate::types::{OrderId, Price};
use rustc_hash::FxHashMap;
#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
use slab::Slab;
use std::collections::BTreeMap;

/// 触发链表节点 (用于侵入式双向链表)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
pub struct TriggerNode {
    /// 指向 condition_order_store 的索引
    pub order_idx: usize,
    /// 链表前驱 (u32::MAX 表示空)
    pub prev: u32,
    /// 链表后继 (u32::MAX 表示空)
    pub next: u32,
}

pub const NULL_NODE: u32 = u32::MAX;

pub struct ConditionOrderManager {
    /// 条件单暂存区 (SL/TP Orders)
    pub(crate) condition_order_store: Slab<OrderData>,

    /// 预分配触发缓冲区 (避免热路径分配，存储 Slab 索引)
    pub(crate) trigger_index_buffer: Vec<usize>,

    /// 侵入式链表节点池
    pub(crate) trigger_node_pool: Slab<TriggerNode>,

    /// 止损触发池 - 买单 (LTP >= TriggerPrice)
    pub(crate) stop_buy_triggers: BTreeMap<Price, u32>,
    /// 止损触发池 - 卖单 (LTP <= TriggerPrice)
    pub(crate) stop_sell_triggers: BTreeMap<Price, u32>,
    /// 止盈触发池 - 买单 (LTP <= TriggerPrice)
    pub(crate) tp_buy_triggers: BTreeMap<Price, u32>,
    /// 止盈触发池 - 卖单 (LTP >= TriggerPrice)
    pub(crate) tp_sell_triggers: BTreeMap<Price, u32>,

    /// 待触发条件单 ID 映射 (OrderId -> (OrderStoreIndex, NodeIndex))
    pub(crate) pending_stop_map: FxHashMap<OrderId, (usize, u32)>,
}

impl Default for ConditionOrderManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConditionOrderManager {
    pub fn new() -> Self {
        Self {
            condition_order_store: Slab::with_capacity(1024),
            trigger_node_pool: Slab::with_capacity(1024),
            trigger_index_buffer: Vec::with_capacity(64),
            stop_buy_triggers: BTreeMap::new(),
            stop_sell_triggers: BTreeMap::new(),
            tp_buy_triggers: BTreeMap::new(),
            tp_sell_triggers: BTreeMap::new(),
            pending_stop_map: FxHashMap::with_capacity_and_hasher(1024, Default::default()),
        }
    }

    pub fn len(&self) -> usize {
        self.condition_order_store.len()
    }

    pub fn is_empty(&self) -> bool {
        self.condition_order_store.is_empty()
    }

    pub fn clear(&mut self) {
        self.condition_order_store.clear();
        self.trigger_node_pool.clear();
        self.stop_buy_triggers.clear();
        self.stop_sell_triggers.clear();
        self.tp_buy_triggers.clear();
        self.tp_sell_triggers.clear();
        self.pending_stop_map.clear();
    }

    /// 注册条件单
    pub fn register_condition_order(&mut self, order: OrderData, ltp: Price) {
        let order_id = order.order_id;
        let trigger_price = order.trigger_price;
        let side = order.side;

        let idx = self.condition_order_store.insert(order);
        let node_idx = self.trigger_node_pool.insert(TriggerNode {
            order_idx: idx,
            prev: NULL_NODE,
            next: NULL_NODE,
        }) as u32;

        self.pending_stop_map.insert(order_id, (idx, node_idx));

        let triggers = match side {
            mt_engine::side::Side::buy => {
                if trigger_price >= ltp {
                    &mut self.stop_buy_triggers
                } else {
                    &mut self.tp_buy_triggers
                }
            }
            mt_engine::side::Side::sell => {
                if trigger_price <= ltp {
                    &mut self.stop_sell_triggers
                } else {
                    &mut self.tp_sell_triggers
                }
            }
            _ => return,
        };

        let entry = triggers.entry(trigger_price).or_insert(NULL_NODE);
        if *entry != NULL_NODE {
            self.trigger_node_pool[node_idx as usize].next = *entry;
            self.trigger_node_pool[*entry as usize].prev = node_idx;
        }
        *entry = node_idx;
    }

    /// 卸载条件单
    pub fn unregister_condition_trigger(&mut self, node_idx: u32, order: &OrderData, ltp: Price) {
        let node = if let Some(n) = self.trigger_node_pool.get(node_idx as usize) {
            *n
        } else {
            return;
        };

        let prev = node.prev;
        let next = node.next;

        if prev != NULL_NODE {
            self.trigger_node_pool[prev as usize].next = next;
        } else {
            let triggers = match order.side {
                mt_engine::side::Side::buy => {
                    if order.trigger_price >= ltp {
                        &mut self.stop_buy_triggers
                    } else {
                        &mut self.tp_buy_triggers
                    }
                }
                mt_engine::side::Side::sell => {
                    if order.trigger_price <= ltp {
                        &mut self.stop_sell_triggers
                    } else {
                        &mut self.tp_sell_triggers
                    }
                }
                _ => return,
            };

            if next == NULL_NODE {
                triggers.remove(&order.trigger_price);
            } else {
                triggers.insert(order.trigger_price, next);
            }
        }

        if next != NULL_NODE {
            self.trigger_node_pool[next as usize].prev = prev;
        }

        self.trigger_node_pool.remove(node_idx as usize);
    }

    pub fn collect_triggered_indices(&mut self, ltp: Price) {
        self.trigger_index_buffer.clear();

        // Buy Stop (LTP >= Trigger)
        while let Some(&first_price) = self.stop_buy_triggers.keys().next() {
            if first_price > ltp {
                break;
            }
            if let Some(head_node_idx) = self.stop_buy_triggers.remove(&first_price) {
                self.collect_and_recycle_trigger_list(head_node_idx);
            }
        }

        // Buy TP (LTP <= Trigger)
        while let Some(&last_price) = self.tp_buy_triggers.keys().next_back() {
            if last_price < ltp {
                break;
            }
            if let Some(head_node_idx) = self.tp_buy_triggers.remove(&last_price) {
                self.collect_and_recycle_trigger_list(head_node_idx);
            }
        }

        // Sell Stop (LTP <= Trigger)
        while let Some(&last_price) = self.stop_sell_triggers.keys().next_back() {
            if last_price < ltp {
                break;
            }
            if let Some(head_node_idx) = self.stop_sell_triggers.remove(&last_price) {
                self.collect_and_recycle_trigger_list(head_node_idx);
            }
        }

        // Sell TP (LTP >= Trigger)
        while let Some(&first_price) = self.tp_sell_triggers.keys().next() {
            if first_price > ltp {
                break;
            }
            if let Some(head_node_idx) = self.tp_sell_triggers.remove(&first_price) {
                self.collect_and_recycle_trigger_list(head_node_idx);
            }
        }
    }

    fn collect_and_recycle_trigger_list(&mut self, head_node_idx: u32) {
        let mut cur = head_node_idx;
        while cur != NULL_NODE {
            let node = if let Some(n) = self.trigger_node_pool.get(cur as usize) {
                *n
            } else {
                break;
            };

            self.prefetch_condition_order(node.order_idx);
            self.trigger_index_buffer.push(node.order_idx);

            let next = node.next;
            self.trigger_node_pool.remove(cur as usize);
            cur = next;
        }
    }

    #[inline(always)]
    fn prefetch_condition_order(&self, idx: usize) {
        if let Some(_entry) = self.condition_order_store.get(idx) {
            #[cfg(target_arch = "x86_64")]
            unsafe {
                use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};
                _mm_prefetch(_entry as *const _ as *const i8, _MM_HINT_T0);
            }
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("prfm pldl1keep, [{0}]", in(reg) _entry);
            }
        }
    }
}
