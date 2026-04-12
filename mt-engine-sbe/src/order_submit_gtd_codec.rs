use crate::*;

pub use decoder::OrderSubmitGtdDecoder;
pub use encoder::OrderSubmitGtdEncoder;

pub use crate::SBE_SCHEMA_ID;
pub use crate::SBE_SCHEMA_VERSION;
pub use crate::SBE_SEMANTIC_VERSION;

pub const SBE_BLOCK_LENGTH: u16 = 72;
pub const SBE_TEMPLATE_ID: u16 = 4;

pub mod encoder {
    use super::*;
    use message_header_codec::*;

    #[derive(Debug, Default)]
    pub struct OrderSubmitGtdEncoder<'a> {
        buf: WriteBuf<'a>,
        initial_offset: usize,
        offset: usize,
        limit: usize,
    }

    impl<'a> Writer<'a> for OrderSubmitGtdEncoder<'a> {
        #[inline]
        fn get_buf_mut(&mut self) -> &mut WriteBuf<'a> {
            &mut self.buf
        }
    }

    impl<'a> Encoder<'a> for OrderSubmitGtdEncoder<'a> {
        #[inline]
        fn get_limit(&self) -> usize {
            self.limit
        }

        #[inline]
        fn set_limit(&mut self, limit: usize) {
            self.limit = limit;
        }
    }

    impl<'a> OrderSubmitGtdEncoder<'a> {
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

        /// primitive field 'order_id'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: OrderId
        /// - encodedOffset: 0
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn order_id(&mut self, value: u64) {
            let offset = self.offset;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'user_id'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: UserId
        /// - encodedOffset: 8
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn user_id(&mut self, value: u64) {
            let offset = self.offset + 8;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// REQUIRED enum
        #[inline]
        pub fn side(&mut self, value: side::Side) {
            let offset = self.offset + 16;
            self.get_buf_mut().put_u8_at(offset, value as u8)
        }

        /// REQUIRED enum
        #[inline]
        pub fn order_type(&mut self, value: order_type::OrderType) {
            let offset = self.offset + 17;
            self.get_buf_mut().put_u8_at(offset, value as u8)
        }

        /// primitive field 'price'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: Price
        /// - encodedOffset: 18
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn price(&mut self, value: u64) {
            let offset = self.offset + 18;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'quantity'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: Quantity
        /// - encodedOffset: 26
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn quantity(&mut self, value: u64) {
            let offset = self.offset + 26;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// REQUIRED enum
        #[inline]
        pub fn time_in_force(&mut self, value: time_in_force::TimeInForce) {
            let offset = self.offset + 34;
            self.get_buf_mut().put_u8_at(offset, value as u8)
        }

        #[inline]
        pub fn flags(&mut self, value: order_flags::OrderFlags) {
            let offset = self.offset + 35;
            self.get_buf_mut().put_u16_at(offset, value.0)
        }

        /// primitive field 'expiry_time'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: ExpiryTime
        /// - encodedOffset: 37
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn expiry_time(&mut self, value: u64) {
            let offset = self.offset + 37;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'timestamp'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: Timestamp
        /// - encodedOffset: 45
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn timestamp(&mut self, value: u64) {
            let offset = self.offset + 45;
            self.get_buf_mut().put_u64_at(offset, value);
        }

        /// primitive field 'sequence_number'
        /// - min value: 0
        /// - max value: -2
        /// - null value: 0xffffffffffffffff_u64
        /// - characterEncoding: null
        /// - semanticType: SequenceNumber
        /// - encodedOffset: 53
        /// - encodedLength: 8
        /// - version: 0
        #[inline]
        pub fn sequence_number(&mut self, value: u64) {
            let offset = self.offset + 53;
            self.get_buf_mut().put_u64_at(offset, value);
        }

    }

} // end encoder

pub mod decoder {
    use super::*;
    use message_header_codec::*;

    #[derive(Clone, Copy, Debug, Default)]
    pub struct OrderSubmitGtdDecoder<'a> {
        buf: ReadBuf<'a>,
        initial_offset: usize,
        offset: usize,
        limit: usize,
        pub acting_block_length: u16,
        pub acting_version: u16,
    }

    impl ActingVersion for OrderSubmitGtdDecoder<'_> {
        #[inline]
        fn acting_version(&self) -> u16 {
            self.acting_version
        }
    }

    impl<'a> Reader<'a> for OrderSubmitGtdDecoder<'a> {
        #[inline]
        fn get_buf(&self) -> &ReadBuf<'a> {
            &self.buf
        }
    }

    impl<'a> Decoder<'a> for OrderSubmitGtdDecoder<'a> {
        #[inline]
        fn get_limit(&self) -> usize {
            self.limit
        }

        #[inline]
        fn set_limit(&mut self, limit: usize) {
            self.limit = limit;
        }
    }

    impl<'a> OrderSubmitGtdDecoder<'a> {
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
        pub fn order_id(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn user_id(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 8)
        }

        /// REQUIRED enum
        #[inline]
        pub fn side(&self) -> side::Side {
            self.get_buf().get_u8_at(self.offset + 16).into()
        }

        /// REQUIRED enum
        #[inline]
        pub fn order_type(&self) -> order_type::OrderType {
            self.get_buf().get_u8_at(self.offset + 17).into()
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn price(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 18)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn quantity(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 26)
        }

        /// REQUIRED enum
        #[inline]
        pub fn time_in_force(&self) -> time_in_force::TimeInForce {
            self.get_buf().get_u8_at(self.offset + 34).into()
        }

        /// BIT SET DECODER
        #[inline]
        pub fn flags(&self) -> order_flags::OrderFlags {
            order_flags::OrderFlags::new(self.get_buf().get_u16_at(self.offset + 35))
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn expiry_time(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 37)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn timestamp(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 45)
        }

        /// primitive field - 'REQUIRED'
        #[inline]
        pub fn sequence_number(&self) -> u64 {
            self.get_buf().get_u64_at(self.offset + 53)
        }

    }

} // end decoder

