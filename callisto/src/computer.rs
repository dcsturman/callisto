use crate::entity::{Vec3, DELTA_TIME, G};
use cgmath::InnerSpace;
use gomez::nalgebra as na;
use gomez::{Domain, Problem, SolverDriver, System};
use na::{Dyn, IsContiguous};

#[derive(Debug)]
pub struct FlightPlan {
    pub path: Vec<Vec3>,
    pub end_velocity: Vec3,
    pub accelerations: Vec<(Vec3, i64)>,
}

// System of equations is represented by a struct.
#[derive(Debug)]
pub struct FlightParams {
    pub start_pos: Vec3,
    pub end_pos: Vec3,
    pub start_vel: Vec3,
    pub end_vel: Vec3,
    pub max_acceleration: f64,
}

impl FlightParams {
    pub fn new(
        start_pos: Vec3,
        end_pos: Vec3,
        start_vel: Vec3,
        end_vel: Vec3,
        max_acceleration: f64,
    ) -> Self {
        FlightParams {
            start_pos,
            end_pos,
            start_vel,
            end_vel,
            max_acceleration,
        }
    }
}

impl Problem for FlightParams {
    // Field type, f32 or f64.
    type Field = f64;

    // Domain of the system.
    fn domain(&self) -> Domain<Self::Field> {
        Domain::unconstrained(8)
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
            - self.end_pos;
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

pub fn compute_flight_path(params: &FlightParams) -> FlightPlan {
    let delta = params.end_pos - params.start_pos;
    let distance = delta.magnitude();

    // Guess initial acceleration as if there was no change in velocity between start and end.
    let guess_accel_1 = delta / distance * params.max_acceleration;
    // Second phase of acceleration is guessed just to be inverse of first.
    let guess_accel_2 = guess_accel_1 * -1.0;

    // Make our starting guess that time is the same in the two phases of acceleration
    let guess_t = (distance / params.max_acceleration).sqrt();

    let array_i: [f64; 3] = guess_accel_1.into();
    let mut initial = Vec::<f64>::from(array_i);
    let array_i_2: [f64; 3] = guess_accel_2.into();
    initial.append(&mut Vec::<f64>::from(array_i_2));
    initial.push(guess_t);
    initial.push(guess_t);

    debug!("(compute_flight_path) Params is {:?}", params);
    debug!("(compute_flight_path) Initial is {:?}", initial);

    let mut solver = SolverDriver::builder(params).with_initial(initial).build();

    let (x, _norm) = solver
        .find(|state| state.norm() <= 1e-6 || state.iter() >= 100)
        .expect("Unable to solve flight path!");

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

    debug!(
        "Computed path with a_1: {:?}, a_2: {:?}, t_1: {:?}, t_2: {:?}",
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

            debug!("(compute_flight_path) Accelerate from {:?} at {:?}m/s^2 for {:?}s. New Pos: {:?}, New Vel: {:?}", 
                pos, accel, delta, new_pos, new_vel);

            path.push(new_pos);
            pos = new_pos;
            vel = new_vel;
            time += delta;
        }
    }

    return FlightPlan {
        path,
        end_velocity: vel,
        accelerations: vec![(a_1 / G, t_1.round() as i64), (a_2 / G, t_2.round() as i64)],
    };
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
}
