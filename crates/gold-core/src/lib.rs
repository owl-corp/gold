//! Pure logic (MIT): no UI, no compositor code.
//!
//! The code keeps a stable structure so it can be compared line by line; these
//! lints would push it away from that.
#![expect(
    clippy::missing_errors_doc,
    reason = "internal app crates, errors are anyhow/ParseError with obvious meaning"
)]
pub mod client_strings;
pub mod data;
pub mod data_update;
pub mod http;
pub mod js;
pub mod leagues;
pub mod parser;
pub mod price_check;
