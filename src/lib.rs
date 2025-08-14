pub(crate) mod cli {
    pub mod control;
    pub mod parser;
    pub mod output;
}
pub(crate) mod storage {
    pub mod config;
    pub mod context;
    pub mod provider;
}
pub(crate) mod client {
    pub mod connect;
    pub mod data;
}
pub(crate) mod error;

pub use crate::cli::control::start_cli;
