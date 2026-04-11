use derive_more::{Add, AddAssign, Display, From, Into, Sub, SubAssign};
use rkyv::{Archive, Deserialize, Serialize};
#[cfg(feature = "serde")]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Price type, precision decided externally (using u64)
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

/// Quantity type, represents order quantity
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

/// Unique Order ID
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

/// Sequence Number, used for ordering and time priority
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

/// Unique User ID
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

/// Timestamp, millisecond precision
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
