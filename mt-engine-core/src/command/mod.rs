use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::order_amend_codec::OrderAmendDecoder;
use mt_engine::order_cancel_codec::OrderCancelDecoder;
use mt_engine::order_flags::OrderFlags;
use mt_engine::order_submit_codec::OrderSubmitDecoder;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;

/// 命令元数据
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMeta {
    pub sequence_number: SequenceNumber,
    pub timestamp: Timestamp,
}

/// 订单提交命令 (Submit)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitCmd {
    pub order_id: OrderId,
    pub user_id: UserId,
    pub side: Side,
    pub order_type: OrderType,
    pub price: Price,
    pub quantity: Quantity,
    pub time_in_force: TimeInForce,
    pub flags: OrderFlags,
    pub expiry: Timestamp,
    pub trigger_price: Price,
    pub visible_qty: Quantity,
}

/// 订单修改命令 (Amend)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendCmd {
    pub order_id: OrderId,
    pub new_price: Price,
    pub new_quantity: Quantity,
}

/// 订单取消命令 (Cancel)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelCmd {
    pub order_id: OrderId,
}

/// 撮合引擎命令类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Submit(SubmitCmd),
    Amend(AmendCmd),
    Cancel(CancelCmd),
}

/// 撮合引擎的标准输入命令
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub meta: CommandMeta,
    pub kind: CommandKind,
}

impl Command {
    /// 从 SBE OrderSubmit 解码器转换为内部 Command
    pub fn from_submit(decoder: &OrderSubmitDecoder) -> Self {
        Self {
            meta: CommandMeta {
                sequence_number: decoder.sequence_number().into(),
                timestamp: decoder.timestamp().into(),
            },
            kind: CommandKind::Submit(SubmitCmd {
                order_id: decoder.order_id().into(),
                user_id: decoder.user_id().into(),
                side: decoder.side(),
                order_type: decoder.order_type(),
                price: decoder.price().into(),
                quantity: decoder.quantity().into(),
                time_in_force: decoder.time_in_force(),
                flags: decoder.flags(),
                expiry: Timestamp(0), // Default, will be overridden if specialized decoder exists
                trigger_price: Price(0),
                visible_qty: Quantity(decoder.quantity()), // Default visible = total
            }),
        }
    }

    /// 从 SBE OrderCancel 解码器转换为内部 Command
    pub fn from_cancel(decoder: &OrderCancelDecoder) -> Self {
        Self {
            meta: CommandMeta {
                sequence_number: decoder.sequence_number().into(),
                timestamp: decoder.timestamp().into(),
            },
            kind: CommandKind::Cancel(CancelCmd {
                order_id: decoder.order_id().into(),
            }),
        }
    }

    /// 从 SBE OrderAmend 解码器转换为内部 Command
    pub fn from_amend(decoder: &OrderAmendDecoder) -> Self {
        Self {
            meta: CommandMeta {
                sequence_number: decoder.sequence_number().into(),
                timestamp: decoder.timestamp().into(),
            },
            kind: CommandKind::Amend(AmendCmd {
                order_id: decoder.order_id().into(),
                new_price: Price(decoder.new_price()),
                new_quantity: Quantity(decoder.new_quantity()),
            }),
        }
    }
}
