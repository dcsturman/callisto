//! What decides a side's orders each round.
//!
//! The skeleton ships with one commander, [`Idle`], which gives no orders: its
//! ships keep the plans they have and the side says Ready. That is enough to
//! prove the plumbing -- a ship switched to AI stops holding up the round --
//! before anything smarter is wired in behind the same trait.

use callisto::entity::Entities;
use callisto::payloads::RequestMsg;

/// One side's decision-maker.
pub trait Commander: Send {
  /// The orders to give this round, as the messages a console would send.
  /// Called once per round, before the side says Ready.
  fn decide(&mut self, world: &Entities, ships: &[String]) -> Vec<RequestMsg>;
}

/// Gives no orders. Ships hold their current plans.
#[derive(Debug, Default)]
pub struct Idle;

impl Commander for Idle {
  fn decide(&mut self, _world: &Entities, _ships: &[String]) -> Vec<RequestMsg> {
    vec![]
  }
}
