#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[repr(u8)]
pub enum OrderType {
    market = 0x0_u8,
    limit = 0x1_u8,
    stop = 0x2_u8,
    stop_limit = 0x3_u8,
    #[default]
    NullVal = 0xff_u8,
}
impl From<u8> for OrderType {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::market,
            0x1_u8 => Self::limit,
            0x2_u8 => Self::stop,
            0x3_u8 => Self::stop_limit,
            _ => Self::NullVal,
        }
    }
}
impl From<OrderType> for u8 {
    #[inline]
    fn from(v: OrderType) -> Self {
        match v {
            OrderType::market => 0x0_u8,
            OrderType::limit => 0x1_u8,
            OrderType::stop => 0x2_u8,
            OrderType::stop_limit => 0x3_u8,
            OrderType::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for OrderType {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "market" => Ok(Self::market),
            "limit" => Ok(Self::limit),
            "stop" => Ok(Self::stop),
            "stop_limit" => Ok(Self::stop_limit),
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for OrderType {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::market => write!(f, "market"),
            Self::limit => write!(f, "limit"),
            Self::stop => write!(f, "stop"),
            Self::stop_limit => write!(f, "stop_limit"),
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
