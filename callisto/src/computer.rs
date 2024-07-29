use crate::entity::{FlightPlan, Vec3, DELTA_TIME, G};
use crate::payloads::Vec3asVec;
use cgmath::{InnerSpace, Zero};
use gomez::nalgebra as na;
use gomez::{Domain, Problem, SolverDriver, System};
use na::{Dyn, IsContiguous};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

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
}

impl Problem for FlightParams {
    // Field type, f32 or f64.
    type Field = f64;

    // Domain of the system.
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

        let pos_eqs = a_1 * t_1 * t_1 / 2.0
            + a_2 * t_2 * t_2 / 2.0
            + (a_1 * t_1 + self.start_vel) * t_2
            + self.start_vel * t_1
            + self.start_pos
            - (self.end_pos
                + if let Some(target_vel) = self.target_velocity {
                    target_vel * ((t_2 + t_1) / DELTA_TIME as f64).ceil() * DELTA_TIME as f64
                } else {
                    Vec3::zero()
                });
        let vel_eqs = a_1 * t_1 + a_2 * t_2 + self.start_vel - self.end_vel;

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

pub fn compute_flight_path(params: &FlightParams) -> FlightPathResult {
    let delta = params.end_pos - params.start_pos;
    let delta_v = params.end_vel - params.start_vel;
    let distance = delta.magnitude();
    let speed = delta_v.magnitude();

    debug!(
        "(compute_flight_path) delta = {:?}, distance3 = {distance}",
        delta
    );
    // Three rougth cases for initial guess:
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
                "iter = {}\t|| r(x) || = {}\tx = {:?}",
                state.iter(),
                state.norm(),
                state.x()
            );
            state.norm() <= SOLVE_TOLERANCE || state.iter() >= 100
        })
        .unwrap_or_else(|e| {
            panic!(
                "Unable to solve flight path with params: {:?} with error: {}.",
                params, e
            )
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
        plan: FlightPlan::new(
            (a_1 / G, t_1.round() as u64).into(),
            Some((a_2 / G, t_2.round() as u64).into()),
        ),
    }
}

pub fn compute_target_path(params: &TargetParams) -> FlightPathResult {
    let delta = params.end_pos - params.start_pos;
    let distance = delta.magnitude();

    let guess_a = delta / distance * params.max_acceleration;
    let guess_t = (2.0 * distance / params.max_acceleration).sqrt();

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
    let answer = if !matches!(attempt, Ok((x, _norm)) if x[3] <= DELTA_TIME as f64) {
        debug!("Second attempt (coudln't get there in one round)");
        let mut initial = Vec::<f64>::from(array_i);
        initial.push(guess_t);
        // We need to compute again since we can't reach the target in one round.
        solver = SolverDriver::builder(params).with_initial(initial).build();
        let (x, _) = solver
            .find(|state| {
                debug!(
                    "iter = {}\t|| r(x) || = {}\tx = {:?}",
                    state.iter(),
                    state.norm(),
                    state.x()
                );
                state.norm() <= SOLVE_TOLERANCE || state.iter() >= 100
            })
            .unwrap_or_else(|e| {
                panic!(
                    "Unable to solve target path with params {:?} and error {}",
                    params, e
                )
            });
        x
    } else {
        let (x, _) = attempt.unwrap();
        x
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
    extern crate pretty_env_logger;
    use pretty_env_logger::env_logger;

    #[test]
    fn test_compute_flight_path() {
        let _ = env_logger::try_init();

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

        let vel_error = (plan.end_velocity - params.end_vel).magnitude();
        let pos_error = (plan.path.last().unwrap() - params.end_pos).magnitude();
        info!("Vel Error: {}\nPos Error: {}", vel_error, pos_error);
        // Add assertions here to validate the computed flight path and velocity
        assert_eq!(plan.path.len(), 5);
        assert!(pos_error < 0.001);
        assert!(vel_error < 0.001);
    }

    #[test]
    fn test_compute_flight_path_with_null_target_velocity() {
        let _ = env_logger::try_init();

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
        assert_eq!(plan.path.len(), 5);
        assert!(pos_error < 0.001);
        assert!(vel_error < 0.001);
    }

    // This test tests a flight path where the first acceleration is less than a round (DELTA_TIME) so the second
    // acceleration is partially applied in each round.
    #[test]
    fn test_compute_flight_short_first_accel() {
        let _ = env_logger::try_init();

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
            max_acceleration: 6.0 * G,
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
        assert_eq!(plan.path.len(), 3);
        assert_eq!(plan.end_velocity, Vec3::zero());
        assert_eq!(plan.path[0], params.start_pos);
        assert_eq!(plan.path[2], params.end_pos);
    }
    #[test]
    fn test_compute_flight_path_with_target_velocity() {
        let _ = env_logger::try_init();

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
        assert_eq!(plan.path.len(), 3);
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

    #[test]
    fn test_compute_flight_path_zero_velocity() {
        let _ = env_logger::try_init();

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
}
