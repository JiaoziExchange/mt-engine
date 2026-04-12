use crate::types::{OrderId, Timestamp};
use mt_engine::message_header_codec;
use mt_engine::message_header_codec::decoder::MessageHeaderDecoder;
use mt_engine::execution_report_codec;
use mt_engine::execution_report_codec::decoder::ExecutionReportDecoder;
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
    /// Get typed execution reports iterator [SAFE READ]
    pub fn execution_reports(&self) -> ExecutionReportIterator<'a> {
        ExecutionReportIterator {
            payload: self.payload,
            offset: 0,
        }
    }
}

/// Typed ExecutionReport Iterator
pub struct ExecutionReportIterator<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Iterator for ExecutionReportIterator<'a> {
    type Item = ExecutionReportDecoder<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.offset + message_header_codec::ENCODED_LENGTH > self.payload.len() {
                return None;
            }

            let header = MessageHeaderDecoder::default().wrap(ReadBuf::new(self.payload), self.offset);
            let template_id = header.template_id();
            let block_length = header.block_length() as usize;
            let msg_size = message_header_codec::ENCODED_LENGTH + block_length;

            if self.offset + msg_size > self.payload.len() {
                return None;
            }

            if template_id == execution_report_codec::SBE_TEMPLATE_ID {
                let decoder = ExecutionReportDecoder::default().header(header, self.offset);
                self.offset += msg_size;
                return Some(decoder);
            } else {
                // Skip other messages (e.g. PublicTrade, DepthUpdate)
                self.offset += msg_size;
            }
        }
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
