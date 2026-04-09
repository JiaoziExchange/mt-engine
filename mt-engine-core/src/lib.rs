//! MT-Engine Core
//! 高性能、确定性的撮合引擎核心实现

#[cfg(all(feature = "snapshot", feature = "dense-node"))]
compile_error!("'snapshot' (exporting) and 'dense-node' features are mutually exclusive to ensure zero-cost on dense nodes. Use 'serde' feature for loading support on dense nodes.");

pub mod book;
pub mod codec;
pub mod command;
pub mod engine;
pub mod orders;
pub mod outcome;
#[cfg(feature = "serde")]
pub mod snapshot;
pub mod types;

pub mod prelude {
    pub use crate::book::backend::OrderBookBackend;
    pub use crate::codec::CommandCodec;
    pub use crate::engine::Engine;
    pub use crate::orders::*;
    pub use crate::outcome::*;
    pub use crate::types::*;
}

#[cfg(test)]
mod tests;
