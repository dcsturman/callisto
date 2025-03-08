use cgmath::{InnerSpace, Zero};
use gomez::nalgebra as na;
use gomez::{Domain, Problem, SolverDriver, System};
use std::error::Error;

use na::{Dyn, IsContiguous};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::entity::{Vec3, DELTA_TIME_F64, G};
use crate::missile::IMPACT_DISTANCE;
use crate::payloads::Vec3asVec;
use crate::ship::FlightPlan;
use crate::{debug, error, info, warn};

// Solver tolerance for typical numerical solving
const SOLVE_TOLERANCE: f64 = 1e-4;
// When the numbers are large sometimes we only get "close".  For this flight computer we're okay
// with close, and define that as 1% - as you get closer you can refine your course and missiles
// refine every round.
const ANS_PERCENT_OFF: f64 = 0.01;
const MAX_ITERATIONS: usize = 400;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct FlightPathResult {
  #[serde_as(as = "Vec<Vec3asVec>")]
  pub path: Vec<Vec3>,
  #[serde_as(as = "Vec3asVec")]
  pub end_velocity: Vec3,
  pub plan: FlightPlan,
}

/**
 * Parameters for this are mostly as you might expect. The starting and ending position, the starting and ending velocity.
 * Max acceleration limits the solution to use no more than this specified acceleration..
 * The target velocity is optional and a bit unusual.  If provided, its the velocity of the target and the end position
 * should be adjusted based on the time of the entire solution plan so that the target is reached at the end of the plan given
 * this velocity.  The other tricky part is while the flight plan likely won't have a duration of an exact number of turns,
 * we should account for the target velocity for a number of full rounds.
 */
#[derive(Debug)]
pub struct FlightParams {
  pub start_pos: Vec3,
  pub end_pos: Vec3,
  pub start_vel: Vec3,
  pub end_vel: Vec3,
  // Can take into account a target velocity instead of just an end position.
  // If we want to do that, then this is Some(target_vel) else None.
  // In this case end_pos is the _current_ end_pos not the ultimate end position.
  pub target_velocity: Option<Vec3>,
  // max_acceleration allowed in m/sec^2 (not G's)
  pub max_acceleration: f64,
}

impl FlightParams {
  pub fn new(
    start_pos: Vec3, end_pos: Vec3, start_vel: Vec3, end_vel: Vec3, target_velocity: Option<Vec3>,
    max_acceleration: f64,
  ) -> Self {
    FlightParams {
      start_pos,
      end_pos,
      start_vel,
      end_vel,
      target_velocity,
      max_acceleration,
    }
  }

  pub fn pos_eq(&self, a_1: Vec3, a_2: Vec3, t_1: f64, t_2: f64) -> Vec3 {
    a_1 * t_1 * t_1 / 2.0
      + a_2 * t_2 * t_2 / 2.0
      + (a_1 * t_1 + self.start_vel) * t_2
      + self.start_vel * t_1
      + self.start_pos
      - (self.end_pos
        + if let Some(target_vel) = self.target_velocity {
          // This line is problematic as its not continuous.
          //target_vel * ((t_2 + t_1) / DELTA_TIME_F64).ceil() * DELTA_TIME_F64
          target_vel * (t_2 + t_1)
        } else {
          Vec3::zero()
        })
  }

  pub fn vel_eq(&self, a_1: Vec3, a_2: Vec3, t_1: f64, t_2: f64) -> Vec3 {
    self.start_vel + a_1 * t_1 + a_2 * t_2 - self.end_vel
  }

  /**
   * Computes a best guess for the acceleration and time at that acceleration.
   * We get more random with each new attempt, but that can help with root solving.
   */
  pub fn best_guess(&self, attempt: u16) -> (Vec3, Vec3, f64, f64) {
    let delta = self.end_pos - self.start_pos;
    let delta_v = self.end_vel - self.start_vel;
    let distance = delta.magnitude();
    let speed = delta_v.magnitude();

    // Three cases.
    // 0) We're not going anywhere.
    // 1) Based on differences in velocity
    // 2) Something to deal with no real movement at all
    // 3) Based on distance.

    // This case is a bit of a hack.  Likely never happens except in tests.
    // I do worry that this is needed points out some other issue but I don't think so.
    if approx::ulps_eq!(distance, 0.0) && approx::ulps_eq!(speed, 0.0) {
      panic!("(best_guess) Should never get this case: Both distance and speed are zero.");
    } else if distance <= speed && speed > 0.0 {
      info!("(best_guess) Making guess based on velocity");
      let accel = delta_v / speed * self.max_acceleration;
      let t_1 = self.start_vel.magnitude() / accel.magnitude() * (1.0 + std::f64::consts::SQRT_2 / 2.0);
      let t_2 = t_1 - self.start_vel.magnitude() / accel.magnitude();
      match attempt {
        0 => (accel, -accel, t_1, t_2),
        1 => (-accel, accel, t_1, t_2),
        2 => (accel, -accel, 1_000_000.0, 1_000_000.0),
        _ => (-accel, accel, -1_000_000.0, -1_000_000.0),
      }
    } else if approx::ulps_eq!(distance, 0.0) {
      info!("(best_guess) Making guess given zero differences.");

      match attempt {
        0 => (delta, -delta, 0.0, 0.0),
        1 => (-delta, delta, 0.0, 0.0),
        2 => (delta, -delta, 1_000_000.0, 1_000_000.0),
        _ => (-delta, delta, -1_000_000.0, -1_000_000.0),
      }
    } else {
      info!("(best_guess) Making guess based on distance.");
      let accel = delta / distance * self.max_acceleration;

      // 0 = 1/2 a * t^2 + v_0 * t - distance
      // t = -v_0 +- sqrt(v_0^2 + 2 * a * distance) / a
      let root_part = (self.start_vel.magnitude().powi(2) + 2.0 * self.max_acceleration * distance).sqrt();

      if root_part < 0.0 {
        error!("(best_guess) Unable to compute best guess.  Root part is negative.");
        return (Vec3::zero(), Vec3::zero(), 0.0, 0.0);
      }

      let (t_a, t_b) = match attempt {
        0 => (
          (-self.start_vel.magnitude() + root_part) / self.max_acceleration,
          (-self.start_vel.magnitude() - root_part) / self.max_acceleration,
        ),
        1 => (1_000_000.0, 1_000_000.0),
        2 => (-1_000_000.0, -1_000_000.0),
        _ => (0.0, 0.0),
      };

      debug!("(best_guess) t_a: {}, t_b: {}", t_a, t_b);
      if t_a > 0.0 {
        (accel, -1.0 * accel, t_a, t_b)
      } else if t_b > 0.0 {
        (accel, -1.0 * accel, t_b, t_b)
      } else {
        error!("(best_guess) Unable to compute best guess.  Both times are negative.");
        (Vec3::zero(), Vec3::zero(), 0.0, 0.0)
      }
    }
  }

  /**
   * Computes a flight path given the parameters.
   * Returns a `FlightPathResult` which contains the path, the end velocity and the plan.
   */
  pub fn compute_flight_path(&self) -> Option<FlightPathResult> {
    // Corner case eliminated here as all these zeros otherwise mess up solution finding.
    if cgmath::ulps_eq!(self.start_pos, self.end_pos) && cgmath::ulps_eq!(self.start_vel, self.end_vel) {
      info!("(compute_flight_path) No need to compute flight path.");
      return Some(FlightPathResult {
        path: vec![self.start_pos],
        end_velocity: self.start_vel,
        plan: FlightPlan::new((Vec3::zero(), 0).into(), Some((Vec3::zero(), 0).into())),
      });
    }

    for attempt in 0..5 {
      info!("(compute_flight_path) Attempt {}", attempt);
      let (guess_accel_1, guess_accel_2, guess_t_1, guess_t_2) = self.best_guess(attempt);
      info!(
        "(compute_flight_path) Guess is a1={:?} a2={:?} t1={:?} t2={:?}",
        guess_accel_1, guess_accel_2, guess_t_1, guess_t_2
      );
      let mut initial: Vec<f64> = Into::<[f64; 3]>::into(guess_accel_1).into();
      initial.append(&mut Into::<[f64; 3]>::into(guess_accel_2).into());
      initial.push(guess_t_1);
      initial.push(guess_t_2);

      info!("(compute_flight_path) Params is {:?}", self);
      info!("(compute_flight_path) Initial is {:?}", initial);

      let mut solver = SolverDriver::builder(self).with_initial(initial).build();
      let solver_result = solver.find(|state| {
        let x = state.x();
        let rx = state.rx();
        info!(
          "iter = {} || |r(x)|={:0.1?} a1={:0.2?} a2={:0.2?} t1={:0.2?} t2={:0.2?} ds={:0.2?} dv={:0.2?} da1={:0.2?} da2={:0.2?}",
          state.iter(),
          state.norm(),
          &x[0..3],
          &x[3..6],
          x[6],
          x[7],
          &rx[0..3],
          &rx[3..6],
          rx[6],
          rx[7],
        );
        state.norm() <= SOLVE_TOLERANCE || state.iter() >= MAX_ITERATIONS
      });

      let (answer, norm) = match solver_result {
        Err(e) => {
          warn!("Unable to solve flight path with params: {self:?} with error: {e}.");
          // This attempt didn't work. On to the next one (and skip all the use of the answer)
          continue;
        }
        Ok(ans) => ans,
      };
      // Unpack the answer.
      let mut a_1 = Vec3::from(
        <&[f64] as TryInto<[f64; 3]>>::try_into(&answer[0..3])
          .expect("(compute_flight_path) Unable to convert to fixed array"),
      );

      let mut a_2 = Vec3::from(
        <&[f64] as TryInto<[f64; 3]>>::try_into(&answer[3..6])
          .expect("(compute_flight_path) Unable to convert to fixed array"),
      );

      let t_1 = answer[6];
      let t_2 = answer[7];

      if norm > SOLVE_TOLERANCE {
        // Case where magnitude of position or velocity are so large, the norm will be a lot larger but still
        // be effectively correct.

        // Check the ratio of how far the calculated position is from the end position to the start position to the end position.
        let pos_percent_off = self.pos_eq(Vec3::from(a_1), Vec3::from(a_2), t_1, t_2).magnitude()
          / (Vec3::from(self.start_pos) - Vec3::from(self.end_pos)).magnitude();
        let vel_percent_off = self.vel_eq(Vec3::from(a_1), Vec3::from(a_2), t_1, t_2).magnitude()
          / (Vec3::from(self.start_vel) - Vec3::from(self.end_vel)).magnitude();
        debug!(
          "(compute_flight_path) Position percent off: {:0.4?}, Velocity percent off: {:0.4?}",
          pos_percent_off, vel_percent_off
        );
        if pos_percent_off > ANS_PERCENT_OFF || vel_percent_off > ANS_PERCENT_OFF {
          warn!("Unable to solve flight path with params: {:?} with norm: {:0.4?}.", self, norm);
          // This attempt didn't work. On to the next one (and skip all the use of the answer)
          continue;
        }
      };

      if t_1 < 0.0 || t_2 < 0.0 {
        warn!("(compute_flight_path) Unable to solve flight path with params: {self:?} with negative time.");
        continue;
      }

      info!("(compute_flight_path) Computed path with a_1: {a_1:?}, a_2: {a_2:?}, t_1: {t_1:?}, t_2: {t_2:?}");

      // Debugging only...
      if a_1.magnitude() > self.max_acceleration || a_2.magnitude() > self.max_acceleration {
        warn!("(compute_flight_path) Path acceleration greater than max.  a_1: {a_1:?}, a_2: {a_2:?}");
        // Trim the accelerations.
        a_1 = a_1.normalize() * (self.max_acceleration - 0.1).max(0.0);
        a_2 = a_2.normalize() * (self.max_acceleration - 0.1).max(0.0);
        info!("(compute_flight_path) Trimmed path to a_1: {a_1:?}, a_2: {a_2:?}");
      }

      let (path, end_velocity) = self.compute_path(&a_1, &a_2, t_1, t_2);

      // Convert time into an unsigned integer.
      #[allow(clippy::cast_possible_truncation)]
      #[allow(clippy::cast_sign_loss)]
      let t_1 = t_1.round() as u64;
      #[allow(clippy::cast_possible_truncation)]
      #[allow(clippy::cast_sign_loss)]
      let t_2 = t_2.round() as u64;

      return Some(FlightPathResult {
        path,
        end_velocity,
        // Convert acceleration back into G's (vs m/s^2) at this point.
        plan: FlightPlan::new((a_1 / G, t_1).into(), Some((a_2 / G, t_2).into())),
      });
    }
    None
  }

  fn compute_path(&self, a_1: &Vec3, a_2: &Vec3, t_1: f64, t_2: f64) -> (Vec<Vec3>, Vec3) {
    // Now that we've solved for acceleration lets create a path and end velocity
    let mut path = Vec::new();
    let mut vel = self.start_vel;
    let mut pos = self.start_pos;

    // Every path starts with the starting position
    path.push(pos);
    for (accel, duration) in [(a_1, t_1), (a_2, t_2)] {
      let mut time = 0.0;
      let mut delta: f64 = DELTA_TIME_F64;
      while approx::relative_ne!(time - duration, 0.0, epsilon = 1e-4) && time < duration {
        if time + delta > duration {
          delta = duration - time;
        }
        let new_pos = pos + vel * delta + accel * delta * delta / 2.0;
        let new_vel = vel + accel * delta;

        info!("(compute_flight_path)\tAccelerate from {:0.0?} at {:0.1?} m/s^2 for {:0.0?}s. New Pos: {:0.0?}, New Vel: {:0.0?}", 
                        pos, accel, delta, new_pos, new_vel);

        path.push(new_pos);
        pos = new_pos;
        vel = new_vel;
        time += delta;
      }
    }
    (path, vel)
  }
}

impl Problem for FlightParams {
  // Field type, f32 or f64.
  type Field = f64;

  // Domain of the system expressed as a rectangular space.
  // The first six values are for each vector (3 values) of acceleration.  Since
  // the max acceleration in the system is 10G or 10*9.807, 100 (or -100) is a valid limit.
  // However they could go a long time so the second two values for duration of each acceleration
  // are without bounds as long as they are positive (cannot have negative time).
  fn domain(&self) -> Domain<Self::Field> {
    Domain::rect(
      vec![-100.0, -100.0, -100.0, -100.0, -100.0, -100.0, 0.0, 0.0],
      vec![100.0, 100.0, 100.0, 100.0, 100.0, 100.0, f64::INFINITY, f64::INFINITY],
    )
  }
}

/**
 * This is the system that is solved to find the flight path.
 * There are 8 inputs into the equations (x).
 * The first three are the first acceleration by x, y, z coordinates.
 * The next three are the second acceleration by x, y, z coordinates.
 * The seventh is the duration (time) of the first acceleration.
 * The eighth is the duration (time) of the second acceleration.
 * The solver will find an x that gives us minimal error.  In particular it finds r(x) where:
 * The first three values of r(x) (by x, y, z) are the difference between the target position and the actual position.
 * The next three values of r(x) (by x, y, z) are the difference between the target velocity and the actual velocity.
 * The seventh value is the difference between the first acceleration magnitude and the max acceleration, squared.
 * The eighth value is the difference between the second acceleration magnitude and the max acceleration, squared.
 */
impl System for FlightParams {
  // Evaluation of the system (computing the residuals).
  fn eval<Sx, Srx>(&self, x: &na::Vector<Self::Field, Dyn, Sx>, rx: &mut na::Vector<Self::Field, Dyn, Srx>)
  where
    Sx: na::storage::Storage<Self::Field, Dyn> + IsContiguous,
    Srx: na::storage::StorageMut<Self::Field, Dyn>,
  {
    // Unpack the values provided
    let a_1: Vec3 = Vec3 {
      x: x[0],
      y: x[1],
      z: x[2],
    };
    let a_2: Vec3 = Vec3 {
      x: x[3],
      y: x[4],
      z: x[5],
    };
    let t_1 = x[6];
    let t_2 = x[7];
    // First the 3 position equations

    let pos_eqs = self.pos_eq(a_1, a_2, t_1, t_2);
    let vel_eqs = self.vel_eq(a_1, a_2, t_1, t_2);

    rx[0] = pos_eqs[0];
    rx[1] = pos_eqs[1];
    rx[2] = pos_eqs[2];
    rx[3] = vel_eqs[0];
    rx[4] = vel_eqs[1];
    rx[5] = vel_eqs[2];
    rx[6] = a_1.dot(a_1) - self.max_acceleration.powi(2);
    rx[7] = a_2.dot(a_2) - self.max_acceleration.powi(2);
  }
}

#[derive(Clone, Debug)]
pub struct TargetParams {
  pub start_pos: Vec3,
  pub end_pos: Vec3,
  pub start_vel: Vec3,
  pub target_vel: Vec3,
  pub max_acceleration: f64,
}

impl TargetParams {
  pub fn new(start_pos: Vec3, end_pos: Vec3, start_vel: Vec3, target_vel: Vec3, max_acceleration: f64) -> Self {
    TargetParams {
      start_pos,
      end_pos,
      start_vel,
      target_vel,
      max_acceleration,
    }
  }

  fn solve(&self, guess: &Vec<f64>) -> Result<Vec<f64>, Box<dyn Error>> {
    info!("(TargetParams.solve) Solving with guess {:?}", guess);
    let mut solver = SolverDriver::builder(self).with_initial(guess.clone()).build();

    let res = solver
      .find(|state| {
        debug!(
          "iter = {}\t|| |r(x)| = {:0.4?}\tx = {:0.2?}\trx = {:0.2?}",
          state.iter(),
          state.norm(),
          state.x(),
          state.rx()
        );
        state.norm() <= SOLVE_TOLERANCE || state.iter() >= MAX_ITERATIONS
      })?
      .0;
    Ok(res.into())
  }

  fn compute_path(&self, answer: &[f64]) -> Vec<Vec3> {
    let mut path = Vec::new();
    let mut vel = self.start_vel;
    let mut pos = self.start_pos;
    let a: Vec3 =
      Vec3::from((<&[f64] as TryInto<[f64; 3]>>::try_into(&answer[0..3])).expect("Unable to convert to fixed array"));

    path.push(pos);
    let mut time = 0.0;
    let mut delta: f64 = DELTA_TIME_F64;
    while time < answer[3] {
      if time + delta > answer[3] {
        delta = answer[3] - time;
      }
      let new_pos = pos + vel * delta + a * delta * delta / 2.0;
      let new_vel = vel + a * delta;
      path.push(new_pos);
      pos = new_pos;
      vel = new_vel;
      time += delta;
    }
    path
  }

  pub fn compute_target_path(&self) -> Option<FlightPathResult> {
    let delta = self.end_pos - self.start_pos;
    let distance = delta.magnitude();

    // Simple but important case where we are launching the missile within impact difference.
    // i.e. it doesn't need to go anywhere.
    if (self.start_pos - self.end_pos).magnitude() < IMPACT_DISTANCE {
      info!("(compute_target_path) No need to compute flight path.");
      return Some(FlightPathResult {
        path: vec![self.start_pos, self.end_pos],
        end_velocity: self.start_vel,
        plan: FlightPlan::new((Vec3::zero(), 0).into(), None),
      });
    }

    // If our guess has any NaN elements its due to distance being zero, so we know that element can be 0.
    // TODO: I don't think this is necessary any more (the check for nan)
    let guess_a = (delta / distance * self.max_acceleration).map(|a| if a.is_nan() { 0.0 } else { a });

    let guess_t = (2.0 * distance / self.max_acceleration).sqrt();
    debug!(
      "(compute_target_path) time guess is {} based on distance = {}, max_accel = {}",
      guess_t, distance, self.max_acceleration
    );

    let mut initial: Vec<f64> = Into::<[f64; 3]>::into(guess_a).into();
    initial.push(guess_t);

    // Our first attempt is if this target can be reached in one round (DELTA_TIME).  In this case,
    // we ignore target velocity.
    let mut first_attempt = self.clone();
    first_attempt.target_vel = Vec3::zero();

    match first_attempt.solve(&initial) {
      Ok(result) if result[3] <= DELTA_TIME_F64 => {
        let a = Vec3::from(
          (<&[f64] as TryInto<[f64; 3]>>::try_into(&result[0..3])).expect("Unable to convert to fixed array"),
        );
        let t = result[3];

        if t < 0.0 {
          warn!("(compute_target_path) First attempt failed. Time is negative.");
          return None;
        }
        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        let t_u64 = t.round() as u64;

        debug!(
          "(compute_target_path) First attempt worked. Acceleration: {:?}, time: {:?}.",
          a, t
        );
        if (self.start_vel + a * t).magnitude() > IMPACT_DISTANCE {
          warn!("(compute_target_path) First attempt worked we might be going too fast to detect impact!");
        }
        Some(FlightPathResult {
          path: first_attempt.compute_path(&result),
          end_velocity: self.start_vel + a * t,
          plan: FlightPlan::new((a / G, t_u64).into(), None),
        })
      }
      Ok(_result) => {
        debug!("Second attempt (couldn't get there in one round) taking into account target velocity.");
        // Now solve with our original params (vs first_attempt)
        self.solve(&initial).map_or_else(
          |e| {
            error!("Unable to solve target path with params {:?} and error {}", self, e);
            None
          },
          |result| {
            let a = Vec3::from(
              (<&[f64] as TryInto<[f64; 3]>>::try_into(&result[0..3])).expect("Unable to convert to fixed array"),
            );
            let t = result[3];

            if t < 0.0 {
              warn!("(compute_target_path) Second attempt failed. Time is negative.");
              return None;
            }

            #[allow(clippy::cast_possible_truncation)]
            #[allow(clippy::cast_sign_loss)]
            let t_u64 = t.round() as u64;
            debug!("(compute_target_path) Second attempt worked.",);
            Some(FlightPathResult {
              path: self.compute_path(&result),
              end_velocity: self.start_vel + a * t,
              plan: FlightPlan::new((a / G, t_u64).into(), None),
            })
          },
        )
      }
      Err(e) => {
        debug!("(compute_target_path) First attempt failed with error {:?}.", e);
        None
      }
    }
  }
}

impl Problem for TargetParams {
  type Field = f64;
  fn domain(&self) -> Domain<Self::Field> {
    Domain::rect(vec![-100.0, -100.0, -100.0, 0.0], vec![100.0, 100.0, 100.0, f64::INFINITY])
  }
}

/**
 * This is the system that is solved to find the flight path for missiles. We are looking
 * to solve for an acceleration and time at that acceleration given the targets velocity and position.
 * There are 4 inputs into the equations (x).
 * The first three are the first acceleration by x, y, z coordinates.
 * The fourth is the time at that acceleration.
 * The solver will find an x that gives us minimal error.  In particular it finds r(x) where:
 * The first three values of r(x) (by x, y, z) are the difference between the target position and the actual position.
 * The eighth value is the difference between the acceleration magnitude and the max acceleration.
 */
impl System for TargetParams {
  fn eval<Sx, Srx>(&self, x: &na::Vector<Self::Field, Dyn, Sx>, rx: &mut na::Vector<Self::Field, Dyn, Srx>)
  where
    Sx: na::storage::Storage<Self::Field, Dyn> + IsContiguous,
    Srx: na::storage::StorageMut<Self::Field, Dyn>,
  {
    let a: Vec3 = Vec3 {
      x: x[0],
      y: x[1],
      z: x[2],
    };
    let t = x[3];

    let pos_eqs = a * t * t / 2.0 + (self.start_vel - self.target_vel) * t + self.start_pos - self.end_pos;

    let a_eq = a.dot(a) - self.max_acceleration.powi(2);

    rx[0] = pos_eqs[0];
    rx[1] = pos_eqs[1];
    rx[2] = pos_eqs[2];
    rx[3] = a_eq;
  }
}

#[cfg(test)]
mod tests {
  use super::super::entity::G;
  use super::*;
  use cgmath::assert_relative_eq;
  use rand::Rng;

  fn pos_error(_start: &Vec3, end: &Vec3, result: &Vec3) -> f64 {
    (end - result).magnitude() / end.magnitude()
  }

  fn vel_error(start: &Vec3, end: &Vec3, result: &Vec3) -> f64 {
    // Velocity error we take as a fraction of the larger of the start or end velocity.
    let bigger = if start.magnitude() > end.magnitude() {
      start.magnitude()
    } else {
      end.magnitude()
    };

    (result - end).magnitude() / bigger
  }

  #[test_log::test]
  fn test_compute_flight_path() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: -2e7,
        y: 1e6,
        z: 1.5e7,
      },
      end_pos: Vec3 {
        x: 1e7,
        y: -2e6,
        z: -2.0e7,
      },
      start_vel: Vec3 {
        x: 500.0,
        y: 0.0,
        z: 0.0,
      },
      end_vel: Vec3 {
        x: 500.0,
        y: 0.0,
        z: 100.0,
      },
      target_velocity: None,
      max_acceleration: 4.0 * G,
    };

    let plan = params.compute_flight_path().unwrap();

    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

    let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
    let p_error = pos_error(&params.start_pos, &params.end_pos, plan.path.last().unwrap());
    info!("Vel Error: {}\nPos Error: {}", v_error, p_error);
    // Add assertions here to validate the computed flight path and velocity
    assert_eq!(plan.path.len(), 7);
    assert!(p_error < 0.001);
    assert!(v_error < 0.001);
  }

  #[test_log::test]
  fn test_compute_flight_path_with_null_target_velocity() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: -2e7,
        y: 1e6,
        z: 1.5e7,
      },
      end_pos: Vec3 {
        x: 1e7,
        y: -2e6,
        z: -2.0e7,
      },
      start_vel: Vec3 {
        x: 500.0,
        y: 0.0,
        z: 0.0,
      },
      end_vel: Vec3 {
        x: 500.0,
        y: 0.0,
        z: 100.0,
      },
      target_velocity: Some(Vec3 { x: 0.0, y: 0.0, z: 0.0 }),
      max_acceleration: 4.0 * G,
    };

    let plan = params.compute_flight_path().unwrap();

    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

    let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
    let p_error = pos_error(&params.start_pos, &params.end_pos, plan.path.last().unwrap());
    info!("Vel Error: {}\nPos Error: {}", v_error, p_error);

    // Add assertions here to validate the computed flight path and velocity
    // Note asserting the path length in this case is kind of weak as we just had
    // to see what value made sense.  The other two tests are more meaningful.
    assert_eq!(plan.path.len(), 7);
    assert!(p_error < 0.001);
    assert!(v_error < 0.001);
  }

  // This test tests a flight path where the first acceleration is less than a round (DELTA_TIME) so the second
  // acceleration is partially applied in each round.
  #[test_log::test]
  fn test_compute_flight_short_first_accel() {
    const MAX_ACCEL: f64 = 6.0 * G;
    let params = FlightParams {
      start_pos: Vec3 {
        x: 7_000_000.0,
        y: -7_000_000.0,
        z: 7_000_000.0,
      },
      end_pos: Vec3 {
        x: 145_738.5,
        y: 39_021_470.2,
        z: 145_738.5,
      },
      start_vel: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
      end_vel: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
      target_velocity: None,
      max_acceleration: MAX_ACCEL,
    };

    let plan = params.compute_flight_path().unwrap();

    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);
    // Use standard physics acceleration equation for a path that full accelerates and then full
    // decelerates to find how long the path should be. (distance = at^2)
    // t = 2*sqrt(distance / a) = 2* sqrt ((start_pos - end_pos).magnitude() / (6.0*G))
    // expected_len = CEIL(t/DELTA_TIME);
    // But we need the start position as well so
    // expected_len = CEIL(t/DELTA_TIME)+2;
    // And one extra for the turn-around datapoint
    // expected_len = CEIL(t/DELTA_TIME)+3;

    let t = 2.0 * ((params.start_pos - params.end_pos).magnitude() / MAX_ACCEL).sqrt();

    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let expected_len = (t / DELTA_TIME_F64).floor() as usize + 3;

    info!(" distance: {}", (params.start_pos - params.end_pos).magnitude());
    info!("Expected len: {}", expected_len);
    info!(" Actual plan: {:?}", plan.plan);

    assert_eq!(plan.path.len(), expected_len);
    assert_relative_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-10);
    assert_relative_eq!(plan.path[0], params.start_pos, epsilon = 1e-10);
    assert_relative_eq!(*plan.path.last().unwrap(), params.end_pos, epsilon = 1e-4);
  }

  #[test_log::test]
  fn test_fast_velocity_compute() {
    let params = FlightParams {
      start_pos: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
      end_pos: Vec3 {
        x: -1e6,
        y: 0.0,
        z: 0.0,
      },
      start_vel: Vec3 { x: 1e4, y: 0.0, z: 0.0 },
      end_vel: Vec3 { x: 1e2, y: 0.0, z: 0.0 },
      target_velocity: None,
      //max_acceleration: 4.0 * G,
      max_acceleration: 40.0,
    };

    let plan = params.compute_flight_path().unwrap();

    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

    let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
    let p_error = pos_error(&params.start_pos, &params.end_pos, plan.path.last().unwrap());

    info!("Vel Error: {}\nPos Error: {}", v_error, p_error);

    assert!(p_error < 0.01);
    assert!(v_error < 0.01);
  }

  #[test_log::test]
  fn test_compute_flight_path_with_target_velocity() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: -2e7,
        y: 1e6,
        z: 1.5e7,
      },
      end_pos: Vec3 {
        x: 1e7,
        y: -2e6,
        z: -2.0e7,
      },
      start_vel: Vec3 {
        x: 500.0,
        y: 0.0,
        z: 0.0,
      },
      end_vel: Vec3 {
        x: 500.0,
        y: 0.0,
        z: 100.0,
      },
      target_velocity: Some(Vec3 {
        x: -1000.0,
        y: 1000.0,
        z: -1000.0,
      }),
      max_acceleration: 6.0 * G,
    };

    let plan = params.compute_flight_path().unwrap();

    #[allow(clippy::cast_precision_loss)]
    //let full_rounds_duration = (plan.plan.duration() as f64 / DELTA_TIME_F64).ceil() * DELTA_TIME_F64;
    let full_rounds_duration = plan.plan.duration() as f64;
    let real_end_target = Vec3 {
      x: params.end_pos.x + params.target_velocity.unwrap().x * full_rounds_duration,
      y: params.end_pos.y + params.target_velocity.unwrap().y * full_rounds_duration,
      z: params.end_pos.z + params.target_velocity.unwrap().z * full_rounds_duration,
    };

    info!(
      "Start Pos: {:?}\nEnd Pos: {:?}\nReal End Pos: {:?}",
      params.start_pos, params.end_pos, real_end_target
    );
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

    let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
    let p_error = pos_error(&params.start_pos, &real_end_target, plan.path.last().unwrap());

    info!("Vel Error: {}\tPos Error: {}", v_error, p_error);
    // Add assertions here to validate the computed flight path and velocity
    assert_eq!(plan.path.len(), 7);
    assert!(
      p_error < 0.001,
      "Pos error is too high ({p_error}).Target position: {:0.0?}, actual position: {:0.0?}",
      real_end_target,
      plan.path.last().unwrap()
    );
    assert!(
      v_error < 0.001,
      "Target velocity: {:0.0?}, actual velocity: {:0.0?}",
      params.end_vel,
      plan.end_velocity
    );
  }

  #[test_log::test]
  fn test_compute_flight_path_zero_velocity() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: 7_000_000.0,
        y: -7_000_000.0,
        z: 7_000_000.0,
      },
      end_pos: Vec3 {
        x: 7_000_000.0,
        y: -7_000_000.0,
        z: 7_000_000.0,
      },
      start_vel: Vec3 {
        x: 6000.0,
        y: 6000.0,
        z: -6000.0,
      },
      end_vel: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
      target_velocity: None,
      max_acceleration: 6.0 * G,
    };

    let plan = params.compute_flight_path().unwrap();

    info!("Start Pos: {:?}\nEnd Pos: {:?}\n", params.start_pos, params.end_pos,);

    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

    let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
    let p_error = pos_error(&params.start_pos, &params.end_pos, plan.path.last().unwrap());

    info!("Vel Error: {}\tPos Error: {}", v_error, p_error);
    // Add assertions here to validate the computed flight path and velocity
    assert_eq!(plan.path.len(), 3);
    assert!(
      p_error < 0.001,
      "Pos error is too high ({p_error}).Target position: {:0.0?}, actual position: {:0.0?}",
      params.end_pos,
      plan.path.last().unwrap()
    );
    assert!(
      v_error < 0.001,
      "Target velocity: {:0.0?}, actual velocity: {:0.0?}",
      params.end_vel,
      plan.end_velocity
    );
  }

  #[test_log::test]
  fn test_compute_flight_path_acceleration_limits() {
    let mut rng = rand::thread_rng();

    for _ in 0..100 {
      // Generate random parameters
      let start_pos = Vec3::new(rng.gen_range(-1e8..1e8), rng.gen_range(-1e8..1e8), rng.gen_range(-1e8..1e8));
      let end_pos = Vec3::new(rng.gen_range(-1e8..1e8), rng.gen_range(-1e8..1e8), rng.gen_range(-1e8..1e8));
      let start_vel = Vec3::new(rng.gen_range(-1e3..1e3), rng.gen_range(-1e3..1e3), rng.gen_range(-1e3..1e3));
      let end_vel = Vec3::new(rng.gen_range(-1e3..1e3), rng.gen_range(-1e3..1e3), rng.gen_range(-1e3..1e3));
      let max_acceleration = rng.gen_range(1.0..10.0) * G;

      let params = FlightParams {
        start_pos,
        end_pos,
        start_vel,
        end_vel,
        target_velocity: None,
        max_acceleration,
      };

      let result = params.compute_flight_path().unwrap();

      // Check that the magnitudes of accelerations are within the limit
      for accel_pair in result.plan.iter() {
        assert!(
          accel_pair.in_limits(params.max_acceleration / G),
          "Acceleration magnitude ({}) exceeds max acceleration ({}) for params: {:?}",
          accel_pair.0.magnitude() * G,
          params.max_acceleration,
          params
        );
      }
    }
  }

  #[test_log::test]
  fn test_compute_flight_plan_to_current_location() {
    // Define current position and velocity
    let current_pos = Vec3::new(1_000_000.0, 2_000_000.0, 3_000_000.0);
    let current_vel = Vec3::new(100.0, 200.0, 300.0);

    // Create FlightParams
    let params = FlightParams {
      start_pos: current_pos,
      end_pos: current_pos, // Same as start_pos
      start_vel: current_vel,
      end_vel: current_vel, // Same as start_vel
      target_velocity: None,
      max_acceleration: 6.0 * G, // Using a typical max acceleration
    };

    // Compute flight path
    let result = params.compute_flight_path().unwrap();

    // Assertions
    assert_eq!(result.path.len(), 1, "Path should only have start point {:?}:", result.path);
    assert_relative_eq!(result.path[0], current_pos);
    assert_relative_eq!(result.end_velocity, current_vel);

    // Check that the flight plan has zero zero duration; acceleration could be anything.
    assert_eq!(result.plan.0 .1, 0);
    if let Some(accel) = &result.plan.1 {
      assert_eq!(accel.1, 0);
    } else {
      panic!("Expecting first acceleration.")
    }

    // Additional check: ensure the plan duration is zero
    assert_eq!(result.plan.duration(), 0);
  }

  // A test that failed to compute a path successfully.  It has the following parameters
  // Start position: [910_933_835.0, 965_592_541.0, -12_291_638.0]
  // End position: [707_200_724.0, 772_000_688.0, -43.69]
  // Start velocity: [-130_149.0, -103,674.0, 7_985.0]
  // End velocity: [2_000.0, 20_000.0, 0.0]
  // Target velocity: [2_000.0, 20_000.0, 0.0]
  // Max acceleration: 58.842
  // This test is passed when the percent difference method on a valid route.
  #[test_log::test]
  fn test_compute_large_absolutes_flight_path() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: 910_933_835.0,
        y: 965_592_541.0,
        z: -12_291_638.0,
      },
      end_pos: Vec3 {
        x: 707_200_724.0,
        y: 772_000_688.0,
        z: -43.69,
      },
      start_vel: Vec3 {
        x: -130_149.0,
        y: -103_674.0,
        z: 7_985.0,
      },
      end_vel: Vec3 {
        x: 2_000.0,
        y: 20_000.0,
        z: 0.0,
      },
      target_velocity: Some(Vec3 {
        x: 2_000.0,
        y: 20_000.0,
        z: 0.0,
      }),
      max_acceleration: 58.842,
    };

    // If we get a result, then the test worked!
    let Some(result) = params.compute_flight_path() else {
      panic!("Unable to compute flight path.");
    };

    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos,);

    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", result.path, result.end_velocity);
  }

  // A test that failed to compute a path successfully.  It has the following parameters
  // Start position: [911_977_932.0, 1_001_160_673.0, -29_410_766.0]
  // End position: [743_210_932.0, 856_298_164.0, -25_419_761.941_245_42],
  // Start velocity: I-108793.25223699937, -66041.76215593825, -6239.6101301463295],
  // End velocity: [20005.0, 46831.0, -14122.0],
  // Target velocity: [20005.0, 46831.0, -14122.0],
  // Max acceleration: 58.842
  // This test is passed when the percent difference method on a valid route.
  #[test_log::test]
  fn test_compute_unsolved() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: 911_977_932.0,
        y: 1_001_160_673.0,
        z: -29_410_766.0,
      },
      end_pos: Vec3 {
        x: 743_210_932.0,
        y: 856_298_164.0,
        z: -25_419_761.941_245_42,
      },
      start_vel: Vec3 {
        x: -108_793.252_236_999_37,
        y: -66_041.762_155_938_25,
        z: -6_239.610_130_146_329_5,
      },
      end_vel: Vec3 {
        x: 20005.0,
        y: 46831.0,
        z: -14122.0,
      },
      target_velocity: Some(Vec3 {
        x: 20005.0,
        y: 46831.0,
        z: -14122.0,
      }),
      max_acceleration: 58.842,
    };

    debug!("D_s={:?}", params.start_pos - params.end_pos);
    debug!("norm_s = {:?}", (params.start_pos - params.end_pos).normalize());
    debug!("D_v={:?}", params.start_vel - params.end_vel);
    debug!("norm_v = {:?}", (params.start_vel - params.end_vel).normalize());

    // If we get a result, then the test worked!
    let Some(result) = params.compute_flight_path() else {
      panic!("Unable to compute flight path.");
    };

    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos,);

    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", result.path, result.end_velocity);
  }

  #[test_log::test]
  fn test_compute_unsolved_2() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: 1_004_140_073.916_692,
        y: 1_054_937_486.251_946_4,
        z: -17_909_755.019_433_156,
      },
      end_pos: Vec3 {
        x: 730_823_285.603_130_8,
        y: 831_711_041.261_878_3,
        z: -16_268_640.810_433_429,
      },
      start_vel: Vec3 {
        x: -136_013.837_557_852_88,
        y: -100_737.856_769_481_92,
        z: 8_396.003_458_726_978,
      },
      end_vel: Vec3 {
        x: 16_404.521_600_000_004,
        y: 41_465.561_600_000_015,
        z: -11_297.664_000_000_002,
      },
      target_velocity: Some(Vec3 {
        x: 16_404.521_600_000_004,
        y: 41_465.561_600_000_015,
        z: -11_297.664_000_000_002,
      }),
      max_acceleration: 58.842,
    };

    debug!("D_s={:?}", params.start_pos - params.end_pos);
    debug!("norm_s = {:?}", (params.start_pos - params.end_pos).normalize());
    debug!("D_v={:?}", params.start_vel - params.end_vel);
    debug!("norm_v = {:?}", (params.start_vel - params.end_vel).normalize());

    // If we get a result, then the test worked!
    let Some(result) = params.compute_flight_path() else {
      panic!("Unable to compute flight path.");
    };
    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos,);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", result.path, result.end_velocity);
  }

  // Another hard case with these parameters:
  // FlightParams {
  //   start_pos: Vector3 [1640287738.7807088, 2784954239.0147567, -346247399.6906175],
  //   end_pos: Vector3 [1991332342.53655, 3373502889.264817, -664233816.1570541],
  //   start_vel: Vector3 [109068.54522619587, 185979.0946917855, -67225.83646958838],
  //   end_vel: Vector3 [-40601.31144820167, -65405.92202694659, 71383.6073708256],
  //   target_velocity: Some(Vector3 [-40601.31144820167, -65405.92202694659, 71383.6073708256]), max_acceleration: 58.842 }
  #[test_log::test]
  fn test_compute_unsolved_3() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: 1_640_287_738.780_708_8,
        y: 2_784_954_239.014_756_7,
        z: -346_247_399.690_617_5,
      },
      end_pos: Vec3 {
        x: 1_991_332_342.536_55,
        y: 3_373_502_889.264_817,
        z: -664_233_816.157_054_1,
      },
      start_vel: Vec3 {
        x: 109_068.545_226_195_87,
        y: 185_979.094_691_785_5,
        z: -67_225.836_469_588_38,
      },
      end_vel: Vec3 {
        x: -40_601.311_448_201_67,
        y: -65_405.922_026_946_59,
        z: 71_383.607_370_825_6,
      },
      target_velocity: Some(Vec3 {
        x: -40_601.311_448_201_67,
        y: -65_405.922_026_946_59,
        z: 71_383.607_370_825_6,
      }),
      max_acceleration: 58.842,
    };

    // If we get a result, then the test worked!
    let Some(result) = params.compute_flight_path() else {
      panic!("Unable to compute flight path.");
    };
    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos,);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", result.path, result.end_velocity);
  }

  // Another test with:
  // FlightParams { start_pos: Vector3 [906717888.588974, 1015658806.334491, -28797065.3460416], 
  // end_pos: Vector3 [746331550.4825101, 892271718.3666573, -26701355.966099627], 
  // start_vel: Vector3 [-109143.90001247398, -57926.015143822726, -6962.14849300116], 
  // end_vel: Vector3 [19299.5, 46478.9, -14828.2], 
  // target_velocity: Some(Vector3 [19299.5, 46478.9, -14828.2]), 
  // max_acceleration: 58.842 }
  #[test_log::test]
  fn test_compute_unsolved_4() {
    let params = FlightParams {
      start_pos: Vec3 {
        x: 906_717_888.588_974,
        y: 1_015_658_806.334_491,
        z: -28_797_065.346_041_6,
      },
      end_pos: Vec3 {
        x: 746_331_550.482_510_1,
        y: 892_271_718.366_657_3,
        z: -26_701_355.966_099_627,
      },
      start_vel: Vec3 {
        x: -109_143.900_012_473_98,
        y: -57_926.015_143_822_726,
        z: -6_962.148_493_001_16,
      },
      end_vel: Vec3 {
        x: 19_299.5,
        y: 46_478.9,
        z: -14_828.2,
      },
      target_velocity: Some(Vec3 {
        x: 19_299.5,
        y: 46_478.9,
        z: -14_828.2,
      }),
      max_acceleration: 58.842,
    };

    // If we get a result, then the test worked!
    let Some(result) = params.compute_flight_path() else {
      panic!("Unable to compute flight path.");
    };
    info!("Start Pos: {:?}\nEnd Pos: {:?}", params.start_pos, params.end_pos,);
    info!("Start Vel: {:?}\nEnd Vel: {:?}", params.start_vel, params.end_vel);
    info!("Path: {:?}\nVel{:?}", result.path, result.end_velocity);
  }
}
