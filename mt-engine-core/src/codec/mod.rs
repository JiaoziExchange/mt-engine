use crate::types::{OrderId, Price, Quantity, SequenceNumber, Timestamp, UserId};
use mt_engine::order_amend_codec::{
    OrderAmendDecoder, OrderAmendEncoder, SBE_BLOCK_LENGTH as AMEND_LEN,
};
use mt_engine::order_cancel_codec::{
    OrderCancelDecoder, OrderCancelEncoder, SBE_BLOCK_LENGTH as CANCEL_LEN,
};
use mt_engine::order_flags::OrderFlags;
use mt_engine::order_submit_codec::{OrderSubmitDecoder, OrderSubmitEncoder, SBE_BLOCK_LENGTH};
use mt_engine::order_type::OrderType;
use mt_engine::side::Side;
use mt_engine::time_in_force::TimeInForce;
use mt_engine::{ReadBuf, WriteBuf};

/// Command Codec - provides safe and consistent API for message construction
pub struct CommandCodec<'a> {
    buffer: &'a mut [u8],
}

impl<'a> CommandCodec<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.buffer
    }

    /// Construct Limit Order Submit message
    #[allow(clippy::too_many_arguments)]
    pub fn encode_submit(
        &mut self,
        offset: usize,
        order_id: OrderId,
        user_id: UserId,
        side: Side,
        price: Price,
        quantity: Quantity,
        seq: SequenceNumber,
        ts: Timestamp,
        tif: TimeInForce,
    ) -> OrderSubmitDecoder<'_> {
        self.encode_submit_ext(
            offset,
            order_id,
            user_id,
            side,
            OrderType::limit,
            price,
            quantity,
            seq,
            ts,
            tif,
            OrderFlags::new(0),
        )
    }

    /// Construct Order Submit message with flags (Extended version)
    #[allow(clippy::too_many_arguments)]
    pub fn encode_submit_ext(
        &mut self,
        offset: usize,
        order_id: OrderId,
        user_id: UserId,
        side: Side,
        order_type: OrderType,
        price: Price,
        quantity: Quantity,
        seq: SequenceNumber,
        ts: Timestamp,
        tif: TimeInForce,
        flags: OrderFlags,
    ) -> OrderSubmitDecoder<'_> {
        let mut encoder =
            OrderSubmitEncoder::default().wrap(WriteBuf::new(&mut self.buffer[offset..]), 0);
        encoder.order_id(order_id.0);
        encoder.user_id(user_id.0);
        encoder.side(side);
        encoder.order_type(order_type);
        encoder.price(price.0);
        encoder.quantity(quantity.0);
        encoder.sequence_number(seq.0);
        encoder.timestamp(ts.0);
        encoder.time_in_force(tif);
        encoder.flags(flags);

        OrderSubmitDecoder::default().wrap(
            ReadBuf::new(&self.buffer[offset..]),
            0,
            SBE_BLOCK_LENGTH,
            mt_engine::SBE_SCHEMA_VERSION,
        )
    }

    /// Construct Market Order Submit message
    #[allow(clippy::too_many_arguments)]
    pub fn encode_market(
        &mut self,
        offset: usize,
        order_id: OrderId,
        user_id: UserId,
        side: Side,
        quantity: Quantity,
        seq: SequenceNumber,
        ts: Timestamp,
    ) -> OrderSubmitDecoder<'_> {
        let mut encoder =
            OrderSubmitEncoder::default().wrap(WriteBuf::new(&mut self.buffer[offset..]), 0);
        encoder.order_id(order_id.0);
        encoder.user_id(user_id.0);
        encoder.side(side);
        encoder.order_type(OrderType::market);
        encoder.price(0); // Market price is 0 in SBE definition
        encoder.quantity(quantity.0);
        encoder.sequence_number(seq.0);
        encoder.timestamp(ts.0);
        encoder.time_in_force(TimeInForce::ioc); // Market defaults to IOC
        encoder.flags(OrderFlags::new(0));

        OrderSubmitDecoder::default().wrap(
            ReadBuf::new(&self.buffer[offset..]),
            0,
            SBE_BLOCK_LENGTH,
            mt_engine::SBE_SCHEMA_VERSION,
        )
    }

    /// Construct Order Submit message with expiry (GTD)
    #[allow(clippy::too_many_arguments)]
    pub fn encode_submit_gtd(
        &mut self,
        offset: usize,
        order_id: OrderId,
        user_id: UserId,
        side: Side,
        price: Price,
        quantity: Quantity,
        seq: SequenceNumber,
        ts: Timestamp,
        expiry: Timestamp,
    ) -> mt_engine::order_submit_gtd_codec::decoder::OrderSubmitGtdDecoder<'_> {
        use mt_engine::order_submit_gtd_codec::{
            OrderSubmitGtdDecoder, OrderSubmitGtdEncoder, SBE_BLOCK_LENGTH as GTD_LEN,
        };
        let mut encoder =
            OrderSubmitGtdEncoder::default().wrap(WriteBuf::new(&mut self.buffer[offset..]), 0);
        encoder.order_id(order_id.0);
        encoder.user_id(user_id.0);
        encoder.side(side);
        encoder.order_type(OrderType::limit);
        encoder.price(price.0);
        encoder.quantity(quantity.0);
        encoder.time_in_force(TimeInForce::gtd);
        encoder.flags(OrderFlags::new(0));
        encoder.expiry_time(expiry.0);
        encoder.timestamp(ts.0);
        encoder.sequence_number(seq.0);

        OrderSubmitGtdDecoder::default().wrap(
            ReadBuf::new(&self.buffer[offset..]),
            0,
            GTD_LEN,
            mt_engine::SBE_SCHEMA_VERSION,
        )
    }

    /// Construct Order Cancel message
    pub fn encode_cancel(
        &mut self,
        offset: usize,
        order_id: OrderId,
        seq: SequenceNumber,
        ts: Timestamp,
    ) -> OrderCancelDecoder<'_> {
        let mut encoder =
            OrderCancelEncoder::default().wrap(WriteBuf::new(&mut self.buffer[offset..]), 0);
        encoder.order_id(order_id.0);
        encoder.sequence_number(seq.0);
        encoder.timestamp(ts.0);

        OrderCancelDecoder::default().wrap(
            ReadBuf::new(&self.buffer[offset..]),
            0,
            CANCEL_LEN,
            mt_engine::SBE_SCHEMA_VERSION,
        )
    }

    /// Construct Order Amend message
    pub fn encode_amend(
        &mut self,
        offset: usize,
        order_id: OrderId,
        new_price: Price,
        new_qty: Quantity,
        seq: SequenceNumber,
        ts: Timestamp,
    ) -> OrderAmendDecoder<'_> {
        let mut encoder =
            OrderAmendEncoder::default().wrap(WriteBuf::new(&mut self.buffer[offset..]), 0);
        encoder.order_id(order_id.0);
        encoder.new_price(new_price.0);
        encoder.new_quantity(new_qty.0);
        encoder.sequence_number(seq.0);
        encoder.timestamp(ts.0);

        OrderAmendDecoder::default().wrap(
            ReadBuf::new(&self.buffer[offset..]),
            0,
            AMEND_LEN,
            mt_engine::SBE_SCHEMA_VERSION,
        )
    }

    /// Construct Control message (Shutdown, etc.)
    pub fn encode_control(
        &mut self,
        offset: usize,
        op: mt_engine::control_op::ControlOp,
        seq: SequenceNumber,
        ts: Timestamp,
    ) -> mt_engine::control_message_codec::decoder::ControlMessageDecoder<'_> {
        use mt_engine::control_message_codec::{
            ControlMessageDecoder, ControlMessageEncoder, SBE_BLOCK_LENGTH as CONTROL_LEN,
        };
        let mut encoder =
            ControlMessageEncoder::default().wrap(WriteBuf::new(&mut self.buffer[offset..]), 0);
        encoder.control_op(op);
        encoder.timestamp(ts.0);
        encoder.sequence_number(seq.0);

        ControlMessageDecoder::default().wrap(
            ReadBuf::new(&self.buffer[offset..]),
            0,
            CONTROL_LEN,
            mt_engine::SBE_SCHEMA_VERSION,
        )
    }
}
