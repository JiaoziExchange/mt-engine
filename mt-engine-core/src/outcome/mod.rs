use crate::types::{OrderId, Timestamp};
use mt_engine::message_header_codec;
use mt_engine::message_header_codec::decoder::MessageHeaderDecoder;
use mt_engine::trade_codec;
use mt_engine::trade_codec::decoder::TradeDecoder;
use mt_engine::ReadBuf;

/// 订单状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Cancelled,
    Rejected,
}

/// 处理结果汇总 (零拷贝版本)
#[derive(Debug)]
pub struct CommandReport<'a> {
    pub order_id: OrderId,
    pub status: OrderStatus,
    pub timestamp: Timestamp,
    pub payload: &'a [u8],
}

impl<'a> CommandReport<'a> {
    /// 获取类型化的成交明细迭代器 [SAFE READ]
    pub fn trades(&self) -> TradeIterator<'a> {
        TradeIterator {
            payload: self.payload,
            offset: 0,
        }
    }
}

/// 类型化成交迭代器
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

/// 命令失败原因
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandFailure {
    SequenceGap,
    OrderNotFound,
    InvalidOrder,
    LiquidityInsufficient,
    PostOnlyViolation,
    Expired,
    InvalidPrice,
}

/// 命令执行结果类型 (零拷贝版本)
#[derive(Debug)]
pub enum CommandOutcome<'a> {
    Applied(CommandReport<'a>),
    Rejected(CommandFailure),
}
