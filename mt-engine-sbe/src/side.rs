#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum Side {
    buy = 0x0_u8,
    sell = 0x1_u8,
    #[default]
    NullVal = 0xff_u8,
}
impl From<u8> for Side {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::buy,
            0x1_u8 => Self::sell,
            _ => Self::NullVal,
        }
    }
}
impl From<Side> for u8 {
    #[inline]
    fn from(v: Side) -> Self {
        match v {
            Side::buy => 0x0_u8,
            Side::sell => 0x1_u8,
            Side::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for Side {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "buy" => Ok(Self::buy),
            "sell" => Ok(Self::sell),
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for Side {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::buy => write!(f, "buy"),
            Self::sell => write!(f, "sell"),
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
