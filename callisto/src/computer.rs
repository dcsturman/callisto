use cgmath::{InnerSpace, Zero};
use gomez::nalgebra as na;
use gomez::{Domain, Problem, SolverDriver, System};

use na::{Dyn, IsContiguous};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use crate::entity::{Vec3, DELTA_TIME, G};
use crate::missile::IMPACT_DISTANCE;
use crate::payloads::Vec3asVec;
use crate::ship::FlightPlan;
use crate::{debug, error, info};

const SOLVE_TOLERANCE: f64 = 1e-4;

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
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
        start_pos: Vec3,
        end_pos: Vec3,
        start_vel: Vec3,
        end_vel: Vec3,
        target_velocity: Option<Vec3>,
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
                    target_vel * ((t_2 + t_1) / DELTA_TIME as f64).ceil() * DELTA_TIME as f64
                } else {
                    Vec3::zero()
                })
    }

    pub fn vel_eq(&self, a_1: Vec3, a_2: Vec3, t_1: f64, t_2: f64) -> Vec3 {
        self.start_vel + a_1 * t_1 + a_2 * t_2 - self.end_vel
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
            vec![
                100.0,
                100.0,
                100.0,
                100.0,
                100.0,
                100.0,
                f64::INFINITY,
                f64::INFINITY,
            ],
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
 * The next three values of r(x) (by x, y, z) are the difference between the target velocity and the actual veloctiy.
 * The seventh value is the difference between the first acceleration magnitude and the max acceleration.
 * The eighth value is the difference between the second acceleration magnitude and the max acceleration.
 */
impl System for FlightParams {
    // Evaluation of the system (computing the residuals).
    fn eval<Sx, Srx>(
        &self,
        x: &na::Vector<Self::Field, Dyn, Sx>,
        rx: &mut na::Vector<Self::Field, Dyn, Srx>,
    ) where
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
        rx[6] = a_1.magnitude() - self.max_acceleration;
        rx[7] = a_2.magnitude() - self.max_acceleration;
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
    pub fn new(
        start_pos: Vec3,
        end_pos: Vec3,
        start_vel: Vec3,
        target_vel: Vec3,
        max_acceleration: f64,
    ) -> Self {
        TargetParams {
            start_pos,
            end_pos,
            start_vel,
            target_vel,
            max_acceleration,
        }
    }
}

impl Problem for TargetParams {
    type Field = f64;
    fn domain(&self) -> Domain<Self::Field> {
        Domain::rect(
            vec![-100.0, -100.0, -100.0, 0.0],
            vec![100.0, 100.0, 100.0, f64::INFINITY],
        )
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
    fn eval<Sx, Srx>(
        &self,
        x: &na::Vector<Self::Field, Dyn, Sx>,
        rx: &mut na::Vector<Self::Field, Dyn, Srx>,
    ) where
        Sx: na::storage::Storage<Self::Field, Dyn> + IsContiguous,
        Srx: na::storage::StorageMut<Self::Field, Dyn>,
    {
        let a: Vec3 = Vec3 {
            x: x[0],
            y: x[1],
            z: x[2],
        };
        let t = x[3];

        let pos_eqs = a * t * t / 2.0 + (self.start_vel - self.target_vel) * t + self.start_pos
            - self.end_pos;
        let a_eq = a.magnitude() - self.max_acceleration;

        rx[0] = pos_eqs[0];
        rx[1] = pos_eqs[1];
        rx[2] = pos_eqs[2];
        rx[3] = a_eq;
    }
}

/**
 * Computes a flight path given the parameters.
 * Returns a FlightPathResult which contains the path, the end velocity and the plan.
 */
pub fn compute_flight_path(params: &FlightParams) -> FlightPathResult {
    let delta = params.end_pos - params.start_pos;
    let delta_v = params.end_vel - params.start_vel;
    let distance = delta.magnitude();
    let speed = delta_v.magnitude();

    debug!(
        "(compute_flight_path) delta = {:?}, distance3 = {distance}",
        delta
    );

    // Three rough cases for initial guess:
    // 1. (most common) mostly correcting for position. Use standard t = at^2
    // 2. Mostly correcting for velocity.
    // 3. Both are near (or at) zero
    let (guess_accel_1, guess_accel_2, guess_t_1, guess_t_2) = if distance <= speed && speed > 0.0 {
        debug!("(compute_flight_path) Making guess based on velocity");
        let accel = delta_v / speed * params.max_acceleration;
        let t_1 = params.start_vel.magnitude() / accel.magnitude()
            * (1.0 + std::f64::consts::SQRT_2 / 2.0);
        let t_2 = t_1 - params.start_vel.magnitude() / accel.magnitude();
        (accel, -1.0 * accel, t_1, t_2)
    } else if distance == 0.0 {
        debug!("(compute_flight_path) Making guess given zero differences.");
        (delta, delta, 0.0, 0.0)
    } else {
        debug!("(compute_flight_path) Making guess based on distance.");
        let accel = delta / distance * params.max_acceleration;

        // Make our starting guess that time is the same in the two phases of acceleration
        let t = (distance / params.max_acceleration).sqrt();

        // Second phase of acceleration is guessed just to be inverse of first.
        (accel, -1.0 * accel, t, t)
    };

    let array_i: [f64; 3] = guess_accel_1.into();
    let mut initial = Vec::<f64>::from(array_i);
    let array_i_2: [f64; 3] = guess_accel_2.into();
    initial.append(&mut Vec::<f64>::from(array_i_2));
    initial.push(guess_t_1);
    initial.push(guess_t_2);

    info!("(compute_flight_path) Params is {:?}", params);
    info!("(compute_flight_path) Initial is {:?}", initial);

    let mut solver = SolverDriver::builder(params).with_initial(initial).build();

    let (x, _norm) = solver
        .find(|state| {
            info!(
                "iter = {}\t|| r(x) || = {}\tx = {:?}\trx = {:?}",
                state.iter(),
                state.norm(),
                state.x(),
                state.rx()
            );
            state.norm() <= SOLVE_TOLERANCE || state.iter() >= 100
        })
        .unwrap_or_else(|e| {
            error!(
                "Unable to solve flight path with params: {:?} with error: {}.",
                params, e
            );
            (&array_i, 0.0)
        });

    let v_a_1: [f64; 3] = x[0..3]
        .try_into()
        .expect("Unable to convert to fixed array");
    let a_1: Vec3 = Vec3::from(v_a_1);

    let v_a_2: [f64; 3] = x[3..6]
        .try_into()
        .expect("Unable to convert to fixed array");
    let a_2: Vec3 = Vec3::from(v_a_2);
    let t_1 = x[6];
    let t_2 = x[7];

    info!(
        "(compute_flight_path) Computed path with a_1: {:?}, a_2: {:?}, t_1: {:?}, t_2: {:?}",
        a_1, a_2, t_1, t_2
    );

    // Now that we've solved for acceleration lets create a path and end velocity
    let mut path = Vec::new();
    let mut vel = params.start_vel;
    let mut pos = params.start_pos;

    // Every path starts with the starting position
    path.push(pos);
    for (accel, duration) in [(a_1, t_1), (a_2, t_2)].iter() {
        let mut time = 0.0;
        let mut delta: f64 = DELTA_TIME as f64;
        while time < *duration {
            if time + delta > *duration {
                delta = *duration - time;
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

    FlightPathResult {
        path,
        end_velocity: vel,
        // Convert acceleration back into G's (vs m/s^2) at this point.
        // Also convert time into an unsigned integer.
        plan: FlightPlan::new(
            (a_1 / G, t_1.round() as u64).into(),
            Some((a_2 / G, t_2.round() as u64).into()),
        ),
    }
}

pub fn compute_target_path(params: &TargetParams) -> FlightPathResult {
    let delta = params.end_pos - params.start_pos;
    let distance = delta.magnitude();

    // Simple but important case where we are launching the missile within impact difference.
    // i.e. it doesn't need to go anywhere.
    if (params.start_pos - params.end_pos).magnitude() < IMPACT_DISTANCE {
        info!("(compute_target_path) No need to compute flight path.");
        return FlightPathResult {
            path: vec![params.start_pos, params.end_pos],
            end_velocity: params.start_vel,
            plan: FlightPlan::new((Vec3::zero(), 0).into(), None),
        };
    }

    // If our guess has any NaN elements its due to distance being zero, so we know that element can be 0.
    let guess_a =
        (delta / distance * params.max_acceleration).map(|a| if a.is_nan() { 0.0 } else { a });

    let guess_t = (2.0 * distance / params.max_acceleration).sqrt();
    debug!(
        "(computer.compute_target_path) time guess is {} based on distance = {}, max_accel = {}",
        guess_t, distance, params.max_acceleration
    );

    let array_i: [f64; 3] = guess_a.into();
    let mut initial = Vec::<f64>::from(array_i);
    initial.push(guess_t);

    // Our first attempt is if this target can be reached in one round (DELTA_TIME).  In this case,
    // we ignore target velocity.
    let mut first_attempt = params.clone();
    first_attempt.target_vel = Vec3::zero();

    info!(
        "(compute_target_path) First attempt params is {:?}",
        first_attempt
    );
    info!(
        "(compute_target_path) First attempt initial is {:?}",
        initial
    );

    let mut solver = SolverDriver::builder(&first_attempt)
        .with_initial(initial)
        .build();

    let attempt = solver.find(|state| {
        info!(
            "iter = {}\t|| r(x) || = {}\tx = {:?}",
            state.iter(),
            state.norm(),
            state.x()
        );
        state.norm() <= SOLVE_TOLERANCE || state.iter() >= 100
    });

    // We need to compute again if either something went wrong in the first attempt (got an error) OR
    // it took too long to reach the target.
    let answer = if !matches!(attempt, Ok((result, _norm)) if result[3] <= DELTA_TIME as f64) {
        debug!(
            "Second attempt (couldn't get there in one round) taking into account target velocity."
        );
        let mut initial = Vec::<f64>::from(array_i);
        initial.push(guess_t);
        // We need to compute again since we can't reach the target in one round.
        // Use the original params without having zero'd out the target velocity.
        solver = SolverDriver::builder(params).with_initial(initial).build();
        solver
            .find(|state| {
                debug!(
                    "iter = {}\t|| r(x) || = {}\tx = {:?}\tr(x) = {:?}",
                    state.iter(),
                    state.norm(),
                    state.x(),
                    state.rx()
                );
                state.norm() <= SOLVE_TOLERANCE || state.iter() >= 100
            })
            .unwrap_or_else(|e| {
                error!(
                    "Unable to solve target path with params {:?} and error {}",
                    params, e
                );
                // Unlikely to be right but better than nothing
                (&array_i, 0.0)
            })
            .0
    } else {
        attempt
            .unwrap_or_else(|e| {
                error!(
                    "Unable to solve target path with params {:?} and error {}",
                    params, e
                );
                (&array_i, 0.0)
            })
            .0
    };

    let mut path = Vec::new();
    let mut vel = params.start_vel;
    let mut pos = params.start_pos;
    let a: Vec3 = Vec3::from(
        (<&[f64] as TryInto<[f64; 3]>>::try_into(&answer[0..3]))
            .expect("Unable to convert to fixed array"),
    );

    path.push(pos);
    let mut time = 0.0;
    let mut delta: f64 = DELTA_TIME as f64;
    while time < answer[3] {
        if time + delta > answer[3] {
            delta = answer[3] - time;
        }
        let new_pos = pos + vel * delta + guess_a * delta * delta / 2.0;
        let new_vel = vel + guess_a * delta;
        path.push(new_pos);
        pos = new_pos;
        vel = new_vel;
        time += delta;
    }

    FlightPathResult {
        path,
        end_velocity: vel,
        plan: FlightPlan::new((a / G, answer[3].round() as u64).into(), None),
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

        let plan = compute_flight_path(&params);

        info!(
            "Start Pos: {:?}\nEnd Pos: {:?}",
            params.start_pos, params.end_pos
        );
        info!(
            "Start Vel: {:?}\nEnd Vel: {:?}",
            params.start_vel, params.end_vel
        );
        info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

        let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
        let p_error = pos_error(
            &params.start_pos,
            &params.end_pos,
            &plan.path.last().unwrap(),
        );
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
            target_velocity: Some(Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            }),
            max_acceleration: 4.0 * G,
        };

        let plan = compute_flight_path(&params);

        info!(
            "Start Pos: {:?}\nEnd Pos: {:?}",
            params.start_pos, params.end_pos
        );
        info!(
            "Start Vel: {:?}\nEnd Vel: {:?}",
            params.start_vel, params.end_vel
        );
        info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

        let vel_error = (plan.end_velocity - params.end_vel).magnitude();
        let pos_error = (plan.path.last().unwrap() - params.end_pos).magnitude();
        info!("Vel Error: {}\nPos Error: {}", vel_error, pos_error);
        // Add assertions here to validate the computed flight path and velocity
        // Note asserting the path length in this case is kind of weak as we just had
        // to see what value made sense.  The other two tests are more meaningful.
        assert_eq!(plan.path.len(), 7);
        assert!(pos_error < 0.001);
        assert!(vel_error < 0.001);
    }

    // This test tests a flight path where the first acceleration is less than a round (DELTA_TIME) so the second
    // acceleration is partially applied in each round.
    #[test_log::test]
    fn test_compute_flight_short_first_accel() {
        const MAX_ACCEL: f64 = 6.0 * G;
        let params = FlightParams {
            start_pos: Vec3 {
                x: 7000000.0,
                y: -7000000.0,
                z: 7000000.0,
            },
            end_pos: Vec3 {
                x: 145738.5,
                y: 39021470.2,
                z: 145738.5,
            },
            start_vel: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            end_vel: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            target_velocity: None,
            max_acceleration: MAX_ACCEL,
        };

        let plan = compute_flight_path(&params);

        info!(
            "Start Pos: {:?}\nEnd Pos: {:?}",
            params.start_pos, params.end_pos
        );
        info!(
            "Start Vel: {:?}\nEnd Vel: {:?}",
            params.start_vel, params.end_vel
        );
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
        let expected_len = (t / DELTA_TIME as f64).floor() as usize + 3;

        info!(
            " distance: {}",
            (params.start_pos - params.end_pos).magnitude()
        );
        info!("Expected len: {}", expected_len);
        info!(" Actual plan: {:?}", plan.plan);

        assert_eq!(plan.path.len(), expected_len);
        assert_relative_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-10);
        assert_relative_eq!(plan.path[0], params.start_pos, epsilon = 1e-10);
        assert_relative_eq!(*plan.path.last().unwrap(), params.end_pos, epsilon = 1e-7);
    }

    #[test_log::test]
    fn test_fast_velocity_compute() {
        let params = FlightParams {
            start_pos: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            end_pos: Vec3 {
                x: -1e6,
                y: 0.0,
                z: 0.0,
            },
            start_vel: Vec3 {
                x: 1e4,
                y: 0.0,
                z: 0.0,
            },
            end_vel: Vec3 {
                x: 1e2,
                y: 0.0,
                z: 0.0,
            },
            target_velocity: None,
            //max_acceleration: 4.0 * G,
            max_acceleration: 40.0,
        };

        let plan = compute_flight_path(&params);

        info!(
            "Start Pos: {:?}\nEnd Pos: {:?}",
            params.start_pos, params.end_pos
        );
        info!(
            "Start Vel: {:?}\nEnd Vel: {:?}",
            params.start_vel, params.end_vel
        );
        info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

        let v_error = vel_error(&params.start_vel, &params.end_vel, &plan.end_velocity);
        let p_error = pos_error(&params.start_pos, &params.end_pos,  plan.path.last().unwrap());

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

        let plan = compute_flight_path(&params);

        let full_rounds_duration =
            (plan.plan.duration() as f64 / DELTA_TIME as f64).ceil() * DELTA_TIME as f64;

        let real_end_target = Vec3 {
            x: params.end_pos.x + params.target_velocity.unwrap().x * full_rounds_duration,
            y: params.end_pos.y + params.target_velocity.unwrap().y * full_rounds_duration,
            z: params.end_pos.z + params.target_velocity.unwrap().z * full_rounds_duration,
        };

        info!(
            "Start Pos: {:?}\nEnd Pos: {:?}\nReal End Pos: {:?}",
            params.start_pos, params.end_pos, real_end_target
        );
        info!(
            "Start Vel: {:?}\nEnd Vel: {:?}",
            params.start_vel, params.end_vel
        );
        info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

        let delta_pos = (params.start_pos - real_end_target).magnitude();
        let delta_v = (params.start_vel - params.end_vel).magnitude();

        let vel_error = (plan.end_velocity - params.end_vel).magnitude() / delta_v;
        let pos_error = (plan.path.last().unwrap() - real_end_target).magnitude() / delta_pos;
        info!("Vel Error: {}\tPos Error: {}", vel_error, pos_error);
        // Add assertions here to validate the computed flight path and velocity
        assert_eq!(plan.path.len(), 7);
        assert!(
            pos_error < 0.001,
            "Pos error is too high ({pos_error}).Target position: {:0.0?}, actual position: {:0.0?}",
            real_end_target,
            plan.path.last().unwrap()
        );
        assert!(
            vel_error < 0.001,
            "Target velocity: {:0.0?}, actual velocity: {:0.0?}",
            params.end_vel,
            plan.end_velocity
        );
    }

    #[test_log::test]
    fn test_compute_flight_path_zero_velocity() {
        let params = FlightParams {
            start_pos: Vec3 {
                x: 7000000.0,
                y: -7000000.0,
                z: 7000000.0,
            },
            end_pos: Vec3 {
                x: 7000000.0,
                y: -7000000.0,
                z: 7000000.0,
            },
            start_vel: Vec3 {
                x: 6000.0,
                y: 6000.0,
                z: -6000.0,
            },
            end_vel: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            target_velocity: None,
            max_acceleration: 6.0 * G,
        };

        let plan = compute_flight_path(&params);

        info!(
            "Start Pos: {:?}\nEnd Pos: {:?}\n",
            params.start_pos, params.end_pos,
        );

        info!(
            "Start Vel: {:?}\nEnd Vel: {:?}",
            params.start_vel, params.end_vel
        );
        info!("Path: {:?}\nVel{:?}", plan.path, plan.end_velocity);

        let delta_v = (params.start_vel - params.end_vel).magnitude();

        let vel_error = (plan.end_velocity - params.end_vel).magnitude() / delta_v;

        let pos_error = (plan.path.last().unwrap() - params.end_pos).magnitude();
        info!("Vel Error: {}\tPos Error: {}", vel_error, pos_error);
        // Add assertions here to validate the computed flight path and velocity
        assert_eq!(plan.path.len(), 3);
        assert!(
            pos_error < 0.001,
            "Pos error is too high ({pos_error}).Target position: {:0.0?}, actual position: {:0.0?}",
            params.end_pos,
            plan.path.last().unwrap()
        );
        assert!(
            vel_error < 0.001,
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
            let start_pos = Vec3::new(
                rng.gen_range(-1e8..1e8),
                rng.gen_range(-1e8..1e8),
                rng.gen_range(-1e8..1e8),
            );
            let end_pos = Vec3::new(
                rng.gen_range(-1e8..1e8),
                rng.gen_range(-1e8..1e8),
                rng.gen_range(-1e8..1e8),
            );
            let start_vel = Vec3::new(
                rng.gen_range(-1e3..1e3),
                rng.gen_range(-1e3..1e3),
                rng.gen_range(-1e3..1e3),
            );
            let end_vel = Vec3::new(
                rng.gen_range(-1e3..1e3),
                rng.gen_range(-1e3..1e3),
                rng.gen_range(-1e3..1e3),
            );
            let max_acceleration = rng.gen_range(1.0..10.0) * G;

            let params = FlightParams {
                start_pos,
                end_pos,
                start_vel,
                end_vel,
                target_velocity: None,
                max_acceleration,
            };

            let result = compute_flight_path(&params);

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
}
