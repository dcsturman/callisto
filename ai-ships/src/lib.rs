//! The AI service: crews AI-controlled ships in callisto.
//!
//! The backend decides nothing for an AI ship. When ships become AI-controlled
//! it asks this service to crew them, one *side* at a time (every AI ship on a
//! team, or one unaligned ship), with `POST /crew`. For each side the service
//! opens a WebSocket to the backend and plays exactly as a human crew would:
//! it logs in, joins the scenario, claims the side's ships, and each round
//! decides, gives its orders and says Ready. `POST /release` ends it.
//!
//! * [`api`] -- the HTTP endpoints the backend calls, and who may call them.
//! * [`crews`] -- one running crew per side, started, updated and stopped.
//! * [`crew`] -- a single side's connection to the backend and its round loop.
//! * [`commander`] -- what decides a side's orders each round.
//! * [`llm`] -- the commander that asks a language model, and [`prompt`],
//!   [`decision`] and [`provider`]: what it is told, how it answers, and the
//!   models it can ask.
//! * [`network`] -- the trained networks that play a side, and how what a
//!   side sees becomes their input and their output becomes orders.
//! * [`observation`] -- what a side may know: fog of war, in one place.
//! * [`auth`] -- the credentials the service presents to the backend.

pub mod api;
pub mod auth;
pub mod cli;
pub mod commander;
pub mod crew;
pub mod crews;
pub mod decision;
pub mod harness;
pub mod llm;
pub mod network;
pub mod observation;
pub mod prompt;
pub mod provider;
