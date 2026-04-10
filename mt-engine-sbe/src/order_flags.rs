#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct OrderFlags(pub u16);
impl OrderFlags {
    #[inline]
    pub fn new(value: u16) -> Self {
        OrderFlags(value)
    }

    #[inline]
    pub fn clear(&mut self) -> &mut Self {
        self.0 = 0;
        self
    }

    #[inline]
    pub fn get_post_only(&self) -> bool {
        0 != self.0 & (1 << 0)
    }

    #[inline]
    pub fn set_post_only(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 0)
        } else {
            self.0 & !(1 << 0)
        };
        self
    }

    #[inline]
    pub fn get_reduce_only(&self) -> bool {
        0 != self.0 & (1 << 1)
    }

    #[inline]
    pub fn set_reduce_only(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 1)
        } else {
            self.0 & !(1 << 1)
        };
        self
    }

    #[inline]
    pub fn get_iceberg(&self) -> bool {
        0 != self.0 & (1 << 2)
    }

    #[inline]
    pub fn set_iceberg(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 2)
        } else {
            self.0 & !(1 << 2)
        };
        self
    }

    #[inline]
    pub fn get_hidden(&self) -> bool {
        0 != self.0 & (1 << 3)
    }

    #[inline]
    pub fn set_hidden(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 3)
        } else {
            self.0 & !(1 << 3)
        };
        self
    }

    #[inline]
    pub fn get_marketable(&self) -> bool {
        0 != self.0 & (1 << 4)
    }

    #[inline]
    pub fn set_marketable(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 4)
        } else {
            self.0 & !(1 << 4)
        };
        self
    }

    #[inline]
    pub fn get_disable_self_trade(&self) -> bool {
        0 != self.0 & (1 << 5)
    }

    #[inline]
    pub fn set_disable_self_trade(&mut self, value: bool) -> &mut Self {
        self.0 = if value {
            self.0 | (1 << 5)
        } else {
            self.0 & !(1 << 5)
        };
        self
    }
}
impl core::fmt::Debug for OrderFlags {
    #[inline]
    fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(fmt, "OrderFlags[post_only(0)={},reduce_only(1)={},iceberg(2)={},hidden(3)={},marketable(4)={},disable_self_trade(5)={}]",
            self.get_post_only(),self.get_reduce_only(),self.get_iceberg(),self.get_hidden(),self.get_marketable(),self.get_disable_self_trade(),)
    }
}
