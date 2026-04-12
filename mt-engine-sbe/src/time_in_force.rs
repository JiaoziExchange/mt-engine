#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
#[repr(u8)]
pub enum TimeInForce {
    ioc = 0x0_u8, 
    fok = 0x1_u8, 
    gtc = 0x2_u8, 
    gtd = 0x3_u8, 
    gth = 0x4_u8, 
    #[default]
    NullVal = 0xff_u8, 
}
impl From<u8> for TimeInForce {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::ioc, 
            0x1_u8 => Self::fok, 
            0x2_u8 => Self::gtc, 
            0x3_u8 => Self::gtd, 
            0x4_u8 => Self::gth, 
            _ => Self::NullVal,
        }
    }
}
impl From<TimeInForce> for u8 {
    #[inline]
    fn from(v: TimeInForce) -> Self {
        match v {
            TimeInForce::ioc => 0x0_u8, 
            TimeInForce::fok => 0x1_u8, 
            TimeInForce::gtc => 0x2_u8, 
            TimeInForce::gtd => 0x3_u8, 
            TimeInForce::gth => 0x4_u8, 
            TimeInForce::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for TimeInForce {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "ioc" => Ok(Self::ioc), 
            "fok" => Ok(Self::fok), 
            "gtc" => Ok(Self::gtc), 
            "gtd" => Ok(Self::gtd), 
            "gth" => Ok(Self::gth), 
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for TimeInForce {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ioc => write!(f, "ioc"), 
            Self::fok => write!(f, "fok"), 
            Self::gtc => write!(f, "gtc"), 
            Self::gtd => write!(f, "gtd"), 
            Self::gth => write!(f, "gth"), 
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
