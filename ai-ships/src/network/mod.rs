//! The trained networks that play a side: how what a side sees becomes their
//! input ([`encode`], [`side`]), what their choices mean as orders
//! ([`action`], [`orders`]), what a side remembers between rounds
//! ([`history`]), the networks themselves ([`net`], [`side_net`]), and the
//! [`commander`]s that put them in a game. They are trained in `rl-env`,
//! which uses this module to play its games.

// Encoding is casts between counts and floats throughout, all of small
// numbers; observations are f32, the engine f64.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

pub mod action;
pub mod commander;
pub mod encode;
pub mod history;
pub mod net;
pub mod orders;
pub mod side;
pub mod side_net;
