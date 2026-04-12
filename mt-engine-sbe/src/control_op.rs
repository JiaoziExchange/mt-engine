#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize))]
#[repr(u8)]
pub enum ControlOp {
    shutdown = 0x0_u8, 
    #[default]
    NullVal = 0xff_u8, 
}
impl From<u8> for ControlOp {
    #[inline]
    fn from(v: u8) -> Self {
        match v {
            0x0_u8 => Self::shutdown, 
            _ => Self::NullVal,
        }
    }
}
impl From<ControlOp> for u8 {
    #[inline]
    fn from(v: ControlOp) -> Self {
        match v {
            ControlOp::shutdown => 0x0_u8, 
            ControlOp::NullVal => 0xff_u8,
        }
    }
}
impl core::str::FromStr for ControlOp {
    type Err = ();

    #[inline]
    fn from_str(v: &str) -> core::result::Result<Self, Self::Err> {
        match v {
            "shutdown" => Ok(Self::shutdown), 
            _ => Ok(Self::NullVal),
        }
    }
}
impl core::fmt::Display for ControlOp {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::shutdown => write!(f, "shutdown"), 
            Self::NullVal => write!(f, "NullVal"),
        }
    }
}
