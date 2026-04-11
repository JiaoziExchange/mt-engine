use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};

pub trait OrderEventListener {
    /// Called when a trade occurs.
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

    // Placeholder for other events (e.g., on_cancel, on_amend) if SBE needs them directly
    // For now, we focus on `on_trade` as it's the core SBE operation in the match loop.
}