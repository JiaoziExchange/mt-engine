use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::order_amend_codec::OrderAmendDecoder;
use mt_engine::order_cancel_codec::OrderCancelDecoder;
use mt_engine::order_flags::OrderFlags;
use mt_engine::order_submit_codec::OrderSubmitDecoder;
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;

/// Command Metadata
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMeta {
    pub sequence_number: SequenceNumber,
    pub timestamp: Timestamp,
}

/// Order Submit Command
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

/// Order Amend Command
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmendCmd {
    pub order_id: OrderId,
    pub new_price: Price,
    pub new_quantity: Quantity,
}

/// Order Cancel Command
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelCmd {
    pub order_id: OrderId,
}

/// Matching Engine Command Kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKind {
    Submit(SubmitCmd),
    Amend(AmendCmd),
    Cancel(CancelCmd),
}

/// Standard input command for the matching engine
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub meta: CommandMeta,
    pub kind: CommandKind,
}

impl Command {
    /// Convert from SBE OrderSubmit decoder to internal Command
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

    /// Convert from SBE OrderCancel decoder to internal Command
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

    /// Convert from SBE OrderAmend decoder to internal Command
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
