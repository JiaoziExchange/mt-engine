use derive_more::{Add, AddAssign, Display, From, Into, Sub, SubAssign};
use rkyv::{Archive, Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// 价格类型，精度由外部决定（统一使用 u64）
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    From,
    Into,
    Add,
    Sub,
    AddAssign,
    SubAssign,
    Display,
    Archive,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
#[archive_attr(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone))]
#[repr(transparent)]
pub struct Price(pub u64);

/// 数量类型，表示订单的委托数量
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    From,
    Into,
    Add,
    Sub,
    AddAssign,
    SubAssign,
    Display,
    Archive,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
#[archive_attr(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone))]
#[repr(transparent)]
pub struct Quantity(pub u64);

/// 订单唯一标识符
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    From,
    Into,
    Display,
    Archive,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
#[archive_attr(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone))]
#[repr(transparent)]
pub struct OrderId(pub u64);

/// 序列号，用于确保命令顺序和实现时间优先级
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    From,
    Into,
    Display,
    Archive,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
#[archive_attr(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone))]
#[repr(transparent)]
pub struct SequenceNumber(pub u64);

/// 用户唯一标识符
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    From,
    Into,
    Display,
    Archive,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
#[archive_attr(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone))]
#[repr(transparent)]
pub struct UserId(pub u64);

/// 时间戳，毫秒精度
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    From,
    Into,
    Display,
    Archive,
    Serialize,
    Deserialize,
)]
#[cfg_attr(feature = "serde", derive(SerdeSerialize, SerdeDeserialize))]
#[archive_attr(derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Copy, Clone))]
#[repr(transparent)]
pub struct Timestamp(pub u64);
