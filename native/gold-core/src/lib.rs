//! Pure logic (MIT): no UI, no compositor code.
//!
//! The code keeps a stable structure so it can be compared line by line; these
//! lints would push it away from that.
#![allow(clippy::neg_multiply, clippy::type_complexity, clippy::if_same_then_else)]
pub mod client_strings;
pub mod data;
pub mod http;
pub mod js;
pub mod leagues;
pub mod parser;
pub mod price_check;
