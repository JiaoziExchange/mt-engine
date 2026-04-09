use crate::*;

pub use decoder::TradeDecoder;
pub use encoder::TradeEncoder;

pub use crate::SBE_SCHEMA_ID;
pub use crate::SBE_SCHEMA_VERSION;
pub use crate::SBE_SEMANTIC_VERSION;

pub const SBE_BLOCK_LENGTH: u16 = 64;
pub const SBE_TEMPLATE_ID: u16 = 101;

pub mod encoder {
    use super::*;
    use message_header_codec::*;

    #[derive(Debug, Default)]
    pub struct TradeEncoder<'a> {
        buf: WriteBuf<'a>,
        initial_offset: usize,
        offset: usize,
        limit: usize,
    }

    impl<'a> Writer<'a> for TradeEncoder<'a> {
        #[inline]
        fn get_buf_mut(&mut self) -> &mut WriteBuf<'a> {
            &mut self.buf
        }
    }

    impl<'a> Encoder<'a> for TradeEncoder<'a> {
        #[inline]
        fn get_limit(&self) -> usize {
            self.limit
        }

        #[inline]
        fn set_limit(&mut self, limit: usize) {
            self.limit = limit;
        }
    }

    impl<'a> TradeEncoder<'a> {
        pub fn wrap(mut self, buf: WriteBuf<'a>, offset: usize) -> Self {
            let limit = offset + SBE_BLOCK_LENGTH as usize;
            self.buf = buf;
            self.initial_offset = offset;
            self.offset = offset;
            self.limit = limit;
            self
        }

        #[inline]
        pub fn encoded_length(&self) -> usize {
            self.limit - self.offset
        }

        pub fn header(self, offset: usize) -> MessageHeaderEncoder<Self> {
            let mut header = MessageHeaderEncoder::default().wrap(self, offset);
            header.block_length(SBE_BLOCK_LENGTH);
            header.template_id(SBE_TEMPLATE_ID);
            header.schema_id(SBE_SCHEMA_ID);
            header.version(SBE_SCHEMA_VERSION);
            header
        }

        /// primitive field 'trade_id'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: OrderId
        /// - encodedOffset: 0
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn trade_id(&mut self, value: u64) {
            let offset = self.offset;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'maker_order_id'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: OrderId
        /// - encodedOffset: 8
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn maker_order_id(&mut self, value: u64) {
            let offset = self.offset + 8;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'taker_order_id'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: OrderId
        /// - encodedOffset: 16
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn taker_order_id(&mut self, value: u64) {
            let offset = self.offset + 16;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// REQUIRED enum
        #[inline]
        pub fn side(&mut self, value: side::Side) {
            let offset = self.offset + 24;
            self.get_buf_mut().put_u8_at(offset, value as u8)
        }

        /// primitive field 'price'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: Price
        /// - encodedOffset: 25
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn price(&mut self, value: u64) {
            let offset = self.offset + 25;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'quantity'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: Quantity
        /// - encodedOffset: 33
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn quantity(&mut self, value: u64) {
            let offset = self.offset + 33;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'timestamp'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: Timestamp
        /// - encodedOffset: 41
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn timestamp(&mut self, value: u64) {
            let offset = self.offset + 41;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'sequence_number'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: SequenceNumber
        /// - encodedOffset: 49
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn sequence_number(&mut self, value: u64) {
            let offset = self.offset + 49;
            self.get_buf_mut().put_u64_at(offset, value);
        }
    }
} // end encoder

pub mod decoder {
    use super::*;
    use message_header_codec::*;

    #[derive(Clone, Copy, Debug, Default)]
    pub struct TradeDecoder<'a> {
        buf: ReadBuf<'a>,
        initial_offset: usize,
        offset: usize,
        limit: usize,
        pub acting_block_length: u16,
        pub acting_version: u16,
    }

    impl ActingVersion for TradeDecoder<'_> {
        #[inline]
        fn acting_version(&self) -> u16 {
            self.acting_version
        }
    }

    impl<'a> Reader<'a> for TradeDecoder<'a> {
        #[inline]
        fn get_buf(&self) -> &ReadBuf<'a> {
            &self.buf
        }
    }

    impl<'a> Decoder<'a> for TradeDecoder<'a> {
        #[inline]
        fn get_limit(&self) -> usize {
            self.limit
        }

        #[inline]
        fn set_limit(&mut self, limit: usize) {
            self.limit = limit;
        }
    }

    impl<'a> TradeDecoder<'a> {
        pub fn wrap(
            mut self,
            buf: ReadBuf<'a>,
            offset: usize,
            acting_block_length: u16,
            acting_version: u16,
        ) -> Self {
            let limit = offset + acting_block_length as usize;
            self.buf = buf;
            self.initial_offset = offset;
            self.offset = offset;
            self.limit = limit;
            self.acting_block_length = acting_block_length;
            self.acting_version = acting_version;
            self
        }

        #[inline]
        pub fn encoded_length(&self) -> usize {
            self.limit - self.offset
        }

        pub fn header(self, mut header: MessageHeaderDecoder<ReadBuf<'a>>, offset: usize) -> Self {
            debug_assert_eq!(SBE_TEMPLATE_ID, header.template_id());
            let acting_block_length = header.block_length();
            let acting_version = header.version();

            self.wrap(
                header.parent().unwrap(),
                offset + message_header_codec::ENCODED_LENGTH,
                acting_block_length,
                acting_version,
            )
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn trade_id(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn maker_order_id(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 8)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn taker_order_id(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 16)
        }

        /// REQUIRED enum
        #[inline]
        pub fn side(&self) -> side::Side {
            self.get_buf().get_u8_at(self.offset + 24).into()
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn price(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 25)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn quantity(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 33)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn timestamp(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 41)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn sequence_number(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 49)
        }
    }
} // end decoder
