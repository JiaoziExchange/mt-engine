use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};

pub trait OrderEventListener {
    /// Called when a trade occurs.
    #[allow(clippy::too_many_arguments)]
    fn on_trade(
        &mut self,
        maker: &OrderData,
        taker: &OrderData,
        trade_qty: Quantity,
        trade_price: Price,
        ts: Timestamp,
        seq: SequenceNumber,
        trade_id: u64,
        offset: &mut usize,
    );

    /// Get payload for CommandReport
    fn get_payload(&self, offset: usize) -> &[u8];

    // Placeholder for other events (e.g., on_cancel, on_amend) if SBE needs them directly
    // For now, we focus on `on_trade` as it's the core SBE operation in the match loop.
}

impl OrderEventListener for () {
    #[inline(always)]
    fn on_trade(
        &mut self,
        _maker: &OrderData,
        _taker: &OrderData,
        _trade_qty: Quantity,
        _trade_price: Price,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _trade_id: u64,
        _offset: &mut usize,
    ) {
    }

    #[inline(always)]
    fn get_payload(&self, _offset: usize) -> &[u8] {
        &[]
    }
}
