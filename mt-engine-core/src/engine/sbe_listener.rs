use crate::engine::events::OrderEventListener;
use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};
use mt_engine::execution_report_codec;
use mt_engine::message_header_codec;
#[cfg(not(feature = "dense-node"))]
use mt_engine::public_trade_codec;
#[cfg(not(feature = "dense-node"))]
use mt_engine::depth_update_codec;
use mt_engine::order_status::OrderStatus as SbeOrderStatus;
use mt_engine::WriteBuf;

pub struct SbeEncoderListener<'a> {
    pub response_buffer: &'a mut [u8],
}

impl<'a> SbeEncoderListener<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            response_buffer: buffer,
        }
    }

    #[inline(always)]
    fn encode_execution_report(
        &mut self,
        order: &OrderData,
        status: SbeOrderStatus,
        ts: Timestamp,
        seq: SequenceNumber,
        offset: &mut usize,
    ) {
        let current_offset = *offset;
        let buf = WriteBuf::new(&mut self.response_buffer[..]);
        let encoder = execution_report_codec::encoder::ExecutionReportEncoder::default().wrap(
            buf,
            current_offset + message_header_codec::ENCODED_LENGTH,
        );
        let mut header_encoder = encoder.header(current_offset);

        header_encoder.block_length(execution_report_codec::SBE_BLOCK_LENGTH);
        header_encoder.template_id(execution_report_codec::SBE_TEMPLATE_ID);
        header_encoder.schema_id(mt_engine::SBE_SCHEMA_ID);
        header_encoder.version(mt_engine::SBE_SCHEMA_VERSION);

        let mut encoder = unsafe { header_encoder.parent().unwrap_unchecked() };
        encoder.order_id(order.order_id.0);
        encoder.user_id(order.user_id.0);
        encoder.status(status);
        encoder.side(order.side);
        encoder.price(order.price.0);
        encoder.quantity(order.remaining_qty.0 + order.filled_qty.0);
        encoder.leaves_qty(order.remaining_qty.0);
        encoder.cum_qty(order.filled_qty.0);
        encoder.timestamp(ts.0);
        encoder.sequence_number(seq.0);

        *offset += message_header_codec::ENCODED_LENGTH + execution_report_codec::SBE_BLOCK_LENGTH as usize;
    }
}

impl<'a> OrderEventListener for SbeEncoderListener<'a> {
    #[inline(always)]
    fn on_accepted(&mut self, order: &OrderData, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        self.encode_execution_report(order, SbeOrderStatus::order_new, ts, seq, offset);
    }

    #[inline(always)]
    fn on_cancelled(&mut self, order: &OrderData, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        self.encode_execution_report(order, SbeOrderStatus::cancelled, ts, seq, offset);
    }

    #[inline(always)]
    fn on_rejected(&mut self, order: &OrderData, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        self.encode_execution_report(order, SbeOrderStatus::rejected, ts, seq, offset);
    }

    #[inline(always)]
    fn on_amended(&mut self, order: &OrderData, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        self.encode_execution_report(order, SbeOrderStatus::order_new, ts, seq, offset);
    }

    #[inline(always)]
    fn on_expired(&mut self, order: &OrderData, ts: Timestamp, seq: SequenceNumber, offset: &mut usize) {
        self.encode_execution_report(order, SbeOrderStatus::expired, ts, seq, offset);
    }

    #[inline(always)]
    fn on_trade(
        &mut self,
        maker: &OrderData,
        taker: &OrderData,
        _trade_qty: Quantity,
        _trade_price: Price,
        ts: Timestamp,
        seq: SequenceNumber,
        _trade_id: u64,
        offset: &mut usize,
    ) {
        self.encode_execution_report(maker, SbeOrderStatus::traded, ts, seq, offset);
        self.encode_execution_report(taker, SbeOrderStatus::traded, ts, seq, offset);

        #[cfg(not(feature = "dense-node"))]
        {
            let current_offset = *offset;
            let buf = WriteBuf::new(&mut self.response_buffer[..]);
            let encoder = public_trade_codec::encoder::PublicTradeEncoder::default().wrap(
                buf,
                current_offset + message_header_codec::ENCODED_LENGTH,
            );
            let mut header_encoder = encoder.header(current_offset);

            header_encoder.block_length(public_trade_codec::SBE_BLOCK_LENGTH);
            header_encoder.template_id(public_trade_codec::SBE_TEMPLATE_ID);
            header_encoder.schema_id(mt_engine::SBE_SCHEMA_ID);
            header_encoder.version(mt_engine::SBE_SCHEMA_VERSION);

            let mut encoder = unsafe { header_encoder.parent().unwrap_unchecked() };
            encoder.trade_id(_trade_id);
            encoder.price(_trade_price.0);
            encoder.quantity(_trade_qty.0);
            encoder.side(taker.side);
            encoder.timestamp(ts.0);
            encoder.sequence_number(seq.0);

            *offset += message_header_codec::ENCODED_LENGTH + public_trade_codec::SBE_BLOCK_LENGTH as usize;
        }
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
        #[cfg(not(feature = "dense-node"))]
        {
            let current_offset = *_offset;
            let buf = WriteBuf::new(&mut self.response_buffer[..]);
            let encoder = depth_update_codec::encoder::DepthUpdateEncoder::default().wrap(
                buf,
                current_offset + message_header_codec::ENCODED_LENGTH,
            );
            let mut header_encoder = encoder.header(current_offset);

            header_encoder.block_length(depth_update_codec::SBE_BLOCK_LENGTH);
            header_encoder.template_id(depth_update_codec::SBE_TEMPLATE_ID);
            header_encoder.schema_id(mt_engine::SBE_SCHEMA_ID);
            header_encoder.version(mt_engine::SBE_SCHEMA_VERSION);

            let mut encoder = unsafe { header_encoder.parent().unwrap_unchecked() };
            encoder.price(_price.0);
            encoder.quantity(_qty.0);
            encoder.side(_side);
            encoder.timestamp(_ts.0);
            encoder.sequence_number(_seq.0);

            *_offset += message_header_codec::ENCODED_LENGTH + depth_update_codec::SBE_BLOCK_LENGTH as usize;
        }
    }

    #[inline(always)]
    fn get_payload(&self, offset: usize) -> &[u8] {
        &self.response_buffer[..offset]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::OrderId;
    use mt_engine::execution_report_codec;
    use mt_engine::message_header_codec;
    use mt_engine::side::Side;
    use mt_engine::ReadBuf;

    #[test]
    fn test_sbe_encoder_listener_on_trade() {
        let mut buffer = vec![0u8; 1024];
        let mut offset = 0;

        let maker = OrderData {
            order_id: OrderId(100),
            side: Side::buy,
            ..Default::default()
        };

        let taker = OrderData {
            order_id: OrderId(101),
            side: Side::sell,
            ..Default::default()
        };

        {
            let mut listener = SbeEncoderListener::new(&mut buffer);

            listener.on_trade(
                &maker,
                &taker,
                Quantity(50),
                Price(12345),
                Timestamp(123456789),
                SequenceNumber(1),
                999,
                &mut offset,
            );
        }

        assert!(offset > 0, "offset should be updated");

        let header = message_header_codec::decoder::MessageHeaderDecoder::default()
            .wrap(ReadBuf::new(&buffer[..]), 0);
        assert_eq!(
            header.template_id(),
            execution_report_codec::SBE_TEMPLATE_ID
        );
        assert_eq!(
            header.block_length(),
            execution_report_codec::SBE_BLOCK_LENGTH
        );
    }
}