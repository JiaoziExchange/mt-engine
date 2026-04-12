use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};

pub trait OrderEventListener {
    /// Called when an order is accepted into the engine.
    fn on_accepted(
        &mut self,
        order: &OrderData,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    );

    /// Called when an order is cancelled.
    fn on_cancelled(
        &mut self,
        order: &OrderData,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    );

    /// Called when an order is rejected.
    fn on_rejected(
        &mut self,
        order: &OrderData,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    );

    /// Called when an order is amended.
    fn on_amended(
        &mut self,
        order: &OrderData,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    );

    /// Called when an order expires.
    fn on_expired(
        &mut self,
        order: &OrderData,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    );

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

    /// Called when the depth (order book) is updated.
    fn on_depth_update(
        &mut self,
        price: Price,
        qty: Quantity,
        side: mt_engine::side::Side,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    );

    /// Get payload for CommandReport
    fn get_payload(&self, offset: usize) -> &[u8];
}

impl OrderEventListener for () {
    #[inline(always)]
    fn on_accepted(
        &mut self,
        _order: &OrderData,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _offset: &mut usize,
    ) {
    }

    #[inline(always)]
    fn on_cancelled(
        &mut self,
        _order: &OrderData,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _offset: &mut usize,
    ) {
    }

    #[inline(always)]
    fn on_rejected(
        &mut self,
        _order: &OrderData,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _offset: &mut usize,
    ) {
    }

    #[inline(always)]
    fn on_amended(
        &mut self,
        _order: &OrderData,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _offset: &mut usize,
    ) {
    }

    #[inline(always)]
    fn on_expired(
        &mut self,
        _order: &OrderData,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _offset: &mut usize,
    ) {
    }

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
    fn on_depth_update(
        &mut self,
        _price: Price,
        _qty: Quantity,
        _side: mt_engine::side::Side,
        _ts: Timestamp,
        _seq: SequenceNumber,
        _offset: &mut usize,
    ) {
    }

    #[inline(always)]
    fn get_payload(&self, _offset: usize) -> &[u8] {
        &[]
    }
}
