use crate::engine::events::OrderEventListener;
use crate::orders::OrderData;
use crate::types::{Price, Quantity, SequenceNumber, Timestamp};
use mt_engine::message_header_codec;
use mt_engine::trade_codec;
use mt_engine::WriteBuf;

pub struct SbeEncoderListener<'a> {
    pub response_buffer: &'a mut [u8],
}

impl<'a> SbeEncoderListener<'a> {
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self { response_buffer: buffer }
    }
}

impl<'a> OrderEventListener for SbeEncoderListener<'a> {
    #[inline(always)]
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
    ) {
        let trade_offset = *offset;
        let trade_buf = WriteBuf::new(&mut self.response_buffer[..]);
        let trade_encoder = trade_codec::encoder::TradeEncoder::default().wrap(
            trade_buf,
            trade_offset + message_header_codec::ENCODED_LENGTH,
        );
        let mut header_encoder = trade_encoder.header(trade_offset);

        header_encoder.block_length(trade_codec::SBE_BLOCK_LENGTH);
        header_encoder.template_id(trade_codec::SBE_TEMPLATE_ID);
        header_encoder.schema_id(mt_engine::SBE_SCHEMA_ID);
        header_encoder.version(mt_engine::SBE_SCHEMA_VERSION);

        let mut trade_encoder = unsafe { header_encoder.parent().unwrap_unchecked() };
        trade_encoder.trade_id(trade_id);
        trade_encoder.maker_order_id(maker.order_id.0);
        trade_encoder.taker_order_id(taker.order_id.0);
        trade_encoder.side(taker.side);
        trade_encoder.price(trade_price.0);
        trade_encoder.quantity(trade_qty.0);
        trade_encoder.timestamp(ts.0);
        trade_encoder.sequence_number(seq.0);

        *offset += message_header_codec::ENCODED_LENGTH + trade_codec::SBE_BLOCK_LENGTH as usize;
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
    use mt_engine::side::Side;
    use mt_engine::trade_codec;
    use mt_engine::message_header_codec;
    use mt_engine::ReadBuf;

    #[test]
    fn test_sbe_encoder_listener_on_trade() {
        let mut buffer = vec![0u8; 1024];
        let mut offset = 0;

        let mut maker = OrderData::default();
        maker.order_id = OrderId(100);
        maker.side = Side::buy;

        let mut taker = OrderData::default();
        taker.order_id = OrderId(101);
        taker.side = Side::sell;

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
        assert_eq!(header.template_id(), trade_codec::SBE_TEMPLATE_ID);
        assert_eq!(header.block_length(), trade_codec::SBE_BLOCK_LENGTH);
    }
}
