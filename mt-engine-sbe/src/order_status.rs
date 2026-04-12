#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[repr(u8)]
pub enum OrderStatus {
    pending = 0x0_u8,
    order_new = 0x1_u8,
    partially_filled = 0x2_u8,
    filled = 0x3_u8,
    cancelled = 0x4_u8,
    rejected = 0x5_u8,
    expired = 0x6_u8,
    traded = 0x7_u8,
    #[default]
    NullVal = 0xff_u8,
}
impl From<u8> for OrderStatus {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::pending,
            0x1_u8 => Self::order_new,
            0x2_u8 => Self::partially_filled,
            0x3_u8 => Self::filled,
            0x4_u8 => Self::cancelled,
            0x5_u8 => Self::rejected,
            0x6_u8 => Self::expired,
            0x7_u8 => Self::traded,
            _ => Self::NullVal,
        }
    }
}
impl From<OrderStatus> for u8 {
    #[inline]
    fn from(v: OrderStatus) -> Self {
        match v {
            OrderStatus::pending => 0x0_u8,
            OrderStatus::order_new => 0x1_u8,
            OrderStatus::partially_filled => 0x2_u8,
            OrderStatus::filled => 0x3_u8,
            OrderStatus::cancelled => 0x4_u8,
            OrderStatus::rejected => 0x5_u8,
            OrderStatus::expired => 0x6_u8,
            OrderStatus::traded => 0x7_u8,
            OrderStatus::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for OrderStatus {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "pending" => Ok(Self::pending),
            "order_new" => Ok(Self::order_new),
            "partially_filled" => Ok(Self::partially_filled),
            "filled" => Ok(Self::filled),
            "cancelled" => Ok(Self::cancelled),
            "rejected" => Ok(Self::rejected),
            "expired" => Ok(Self::expired),
            "traded" => Ok(Self::traded),
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for OrderStatus {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::pending => write!(f, "pending"),
            Self::order_new => write!(f, "order_new"),
            Self::partially_filled => write!(f, "partially_filled"),
            Self::filled => write!(f, "filled"),
            Self::cancelled => write!(f, "cancelled"),
            Self::rejected => write!(f, "rejected"),
            Self::expired => write!(f, "expired"),
            Self::traded => write!(f, "traded"),
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
