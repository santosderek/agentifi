//! Reusable visual primitives.
//!
//! Components own no routing or network logic; views compose them.

pub mod activity_timeline;
pub mod board_card;
pub mod empty_state;
pub mod icon;
pub mod inspector;
pub mod logo;
pub mod navigation_rail;
pub mod prompt_composer;
pub mod session_row;
pub mod stat;
pub mod status_badge;
pub mod text;
pub mod toast;
pub mod toolbar;

pub use activity_timeline::{activity_feed, activity_timeline};
pub use board_card::board_card;
pub use empty_state::{empty_state, empty_state_action};
pub use icon::{icon_button, paint, Icon};
pub use logo::agentifi_mark;
pub use navigation_rail::{nav_item, rail_connection, rail_project};
pub use prompt_composer::{prompt_composer, ComposerAction};
pub use session_row::{session_row, RowStyle};
pub use stat::stat;
pub use toast::toast_layer;
