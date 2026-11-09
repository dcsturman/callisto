//! The callisto engine as a reinforcement-learning environment.
//!
//! [`batch::Batch`] steps many games at once on a thread pool; each
//! [`game::Game`] is one scenario between two sides, played by the network
//! or by a baseline; [`encode`] turns what a ship sees into a fixed-length
//! vector and [`action`] turns the network's choices back into orders. The
//! Python module (feature `python`) is a thin layer over the batch.
//!
//! See `docs/rl_plan.md` for the why.

pub mod batch;
pub mod game;
pub mod generate;
#[cfg(feature = "python")]
mod python;
pub mod side_batch;
// What plays a game -- the encoders, the networks and their commanders --
// lives in the AI service, which plays real games with it; training uses it
// from there.
pub use ai_ships::network::{action, commander, encode, history, net, orders, side, side_net};
