use crate::types::{OrderId, Timestamp};
use mt_engine::message_header_codec;
use mt_engine::message_header_codec::decoder::MessageHeaderDecoder;
use mt_engine::trade_codec;
use mt_engine::trade_codec::decoder::TradeDecoder;
use mt_engine::ReadBuf;

/// Order Status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// Command Execution Report (Zero-copy version)
#[derive(Debug)]
pub struct CommandReport<'a> {
    pub order_id: OrderId,
    pub status: OrderStatus,
    pub timestamp: Timestamp,
    pub payload: &'a [u8],
}

impl<'a> CommandReport<'a> {
    /// Get typed trade details iterator [SAFE READ]
    pub fn trades(&self) -> TradeIterator<'a> {
        TradeIterator {
            payload: self.payload,
            offset: 0,
        }
    }
}

/// Typed Trade Iterator
pub struct TradeIterator<'a> {
    payload: &'a [u8],
    offset: usize,
}

const TRADE_MESSAGE_SIZE: usize =
    message_header_codec::ENCODED_LENGTH + trade_codec::SBE_BLOCK_LENGTH as usize;

impl<'a> Iterator for TradeIterator<'a> {
    type Item = TradeDecoder<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset + TRADE_MESSAGE_SIZE > self.payload.len() {
            return None;
        }

        let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(self.payload), self.offset);
        let decoder = TradeDecoder::default().header(header, self.offset);
        self.offset += TRADE_MESSAGE_SIZE;
        Some(decoder)
    }
}

/// Command failure reasons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandFailure {
    SequenceGap,
    OrderNotFound,
    InvalidOrder,
    LiquidityInsufficient,
    PostOnlyViolation,
    Expired,
    InvalidPrice,
    DuplicateOrderId,
    CapacityExceeded,
    InvalidOrderId,
    SystemHalted,
}

/// Command execution outcome type (Zero-copy version)
#[derive(Debug)]
pub enum CommandOutcome<'a> {
    Applied(CommandReport<'a>),
    Rejected(CommandFailure),
}
