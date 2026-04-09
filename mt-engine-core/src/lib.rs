//! MT-Engine Core
//! 高性能、确定性的撮合引擎核心实现

pub mod book;
pub mod codec;
pub mod command;
pub mod engine;
pub mod orders;
pub mod outcome;
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
