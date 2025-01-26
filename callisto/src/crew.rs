use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skills {
    Pilot,
    EngineeringJump,
    EngineeringPower,
    EngineeringManeuver,
    Gunnery,
    Sensors,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Crew {
    #[serde(default)]
    pilot: u8,
    #[serde(default)]
    engineering_jump: u8,
    #[serde(default)]
    engineering_power: u8,
    #[serde(default)]
    engineering_maneuver: u8,
    #[serde(default)]
    sensors: u8,
    #[serde(default = "default_gunnery")]
    gunnery: Vec<u8>,
}

// Function just to provide a default value for gunnery deserialization
fn default_gunnery() -> Vec<u8> {
    vec![]
}

impl Crew {
    #[must_use]
    pub fn new() -> Crew {
        Crew {
            pilot: 0,
            engineering_jump: 0,
            engineering_power: 0,
            engineering_maneuver: 0,
            sensors: 0,
            gunnery: vec![],
        }
    }

    /// Get the skill level for a crew for a particular skill.  Note that you cannot
    /// use this function to get gunnery skill as there may be multiple gunners in the crew. Use
    /// `get_gunnery` instead.
    ///
    /// # Arguments
    /// * `skill` - The skill to get the level for.
    ///
    /// # Panics
    /// Panics if the skill is gunnery.
    #[must_use]
    pub fn get_skill(&self, skill: Skills) -> u8 {
        match skill {
            Skills::Pilot => self.pilot,
            Skills::EngineeringJump => self.engineering_jump,
            Skills::EngineeringPower => self.engineering_power,
            Skills::EngineeringManeuver => self.engineering_maneuver,
            Skills::Sensors => self.sensors,
            Skills::Gunnery => panic!("(Crew.getSkill) Multiple gunners possible."),
        }
    }

    #[must_use]
    pub fn get_pilot(&self) -> u8 {
        self.pilot
    }

    #[must_use]
    pub fn get_engineering_jump(&self) -> u8 {
        self.engineering_jump
    }

    #[must_use]
    pub fn get_engineering_power(&self) -> u8 {
        self.engineering_power
    }

    #[must_use]
    pub fn get_engineering_maneuver(&self) -> u8 {
        self.engineering_maneuver
    }

    #[must_use]
    pub fn get_sensors(&self) -> u8 {
        self.sensors
    }

    #[must_use]
    pub fn get_gunnery(&self, gun: usize) -> u8 {
        if gun >= self.gunnery.len() {
            return 0;
        }
        self.gunnery[gun]
    }

    /// Sets a crew skill level.  Note that setting a skill this way for gunnery is not allowed.
    /// Instead use `add_gunnery`.
    ///
    /// # Arguments
    /// * `skill` - The skill to set.
    /// * `value` - The value to set the skill to.
    ///
    /// # Panics
    /// Panics if the skill is gunnery.
    pub fn set_skill(&mut self, skill: Skills, value: u8) {
        match skill {
            Skills::Pilot => self.pilot = value,
            Skills::EngineeringJump => self.engineering_jump = value,
            Skills::EngineeringPower => self.engineering_power = value,
            Skills::EngineeringManeuver => self.engineering_maneuver = value,
            Skills::Sensors => self.sensors = value,
            Skills::Gunnery => panic!("Cannot use set_skill for gunnery. Use add_gunnery instead."),
        }
    }

    pub fn add_gunnery(&mut self, value: u8) {
        self.gunnery.push(value);
    }
}

impl Default for Crew {
    fn default() -> Self {
        Crew::new()
    }
}

// Add this at the end of your crew.rs file

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_crew_new() {
        let crew = Crew::new();
        assert_eq!(crew.pilot, 0);
        assert_eq!(crew.engineering_jump, 0);
        assert_eq!(crew.engineering_power, 0);
        assert_eq!(crew.engineering_maneuver, 0);
        assert_eq!(crew.sensors, 0);
        assert_eq!(crew.gunnery, Vec::<u8>::new());
    }

    #[test_log::test]
    fn test_get_skill() {
        let mut crew = Crew::new();
        crew.pilot = 3;
        crew.engineering_jump = 2;
        crew.engineering_power = 1;
        crew.engineering_maneuver = 4;
        crew.sensors = 5;

        assert_eq!(crew.get_skill(Skills::Pilot), 3);
        assert_eq!(crew.get_skill(Skills::EngineeringJump), 2);
        assert_eq!(crew.get_skill(Skills::EngineeringPower), 1);
        assert_eq!(crew.get_skill(Skills::EngineeringManeuver), 4);
        assert_eq!(crew.get_skill(Skills::Sensors), 5);
    }

    #[test_log::test]
    #[should_panic(expected = "(Crew.getSkill) Multiple gunners possible.")]
    fn test_get_skill_gunnery_panic() {
        let crew = Crew::new();
        let _ = crew.get_skill(Skills::Gunnery);
    }

    #[test_log::test]
    fn test_get_individual_skills() {
        let mut crew = Crew::new();
        crew.pilot = 3;
        crew.engineering_jump = 2;
        crew.engineering_power = 1;
        crew.engineering_maneuver = 4;
        crew.sensors = 5;

        assert_eq!(crew.get_pilot(), 3);
        assert_eq!(crew.get_engineering_jump(), 2);
        assert_eq!(crew.get_engineering_power(), 1);
        assert_eq!(crew.get_engineering_maneuver(), 4);
        assert_eq!(crew.get_sensors(), 5);
    }

    #[test_log::test]
    fn test_get_gunnery() {
        let mut crew = Crew::new();
        crew.gunnery = vec![1, 2, 3];

        assert_eq!(crew.get_gunnery(0), 1);
        assert_eq!(crew.get_gunnery(1), 2);
        assert_eq!(crew.get_gunnery(2), 3);
        assert_eq!(crew.get_gunnery(3), 0); // Out of range
    }

    #[test_log::test]
    fn test_set_skill() {
        let mut crew = Crew::new();

        crew.set_skill(Skills::Pilot, 3);
        crew.set_skill(Skills::EngineeringJump, 2);
        crew.set_skill(Skills::EngineeringPower, 1);
        crew.set_skill(Skills::EngineeringManeuver, 4);
        crew.set_skill(Skills::Sensors, 5);

        assert_eq!(crew.pilot, 3);
        assert_eq!(crew.engineering_jump, 2);
        assert_eq!(crew.engineering_power, 1);
        assert_eq!(crew.engineering_maneuver, 4);
        assert_eq!(crew.sensors, 5);
    }

    #[test_log::test]
    #[should_panic(expected = "Cannot use set_skill for gunnery. Use add_gunnery instead.")]
    fn test_set_skill_gunnery_panic() {
        let mut crew = Crew::new();
        crew.set_skill(Skills::Gunnery, 1);
    }

    #[test_log::test]
    fn test_add_gunnery() {
        let mut crew = Crew::new();

        crew.add_gunnery(1);
        crew.add_gunnery(2);
        crew.add_gunnery(3);

        assert_eq!(crew.gunnery, vec![1, 2, 3]);
    }

    #[test_log::test]
    fn test_default_gunnery() {
        let default_gunnery = default_gunnery();
        assert_eq!(default_gunnery, Vec::<u8>::new());
    }

    #[test_log::test]
    fn test_crew_serialization_deserialization() {
        let mut crew = Crew::new();
        crew.pilot = 3;
        crew.engineering_jump = 2;
        crew.engineering_power = 1;
        crew.engineering_maneuver = 4;
        crew.sensors = 5;
        crew.gunnery = vec![1, 2, 3];

        let serialized = serde_json::to_string(&crew).unwrap();
        let deserialized: Crew = serde_json::from_str(&serialized).unwrap();

        assert_eq!(crew.pilot, deserialized.pilot);
        assert_eq!(crew.engineering_jump, deserialized.engineering_jump);
        assert_eq!(crew.engineering_power, deserialized.engineering_power);
        assert_eq!(crew.engineering_maneuver, deserialized.engineering_maneuver);
        assert_eq!(crew.sensors, deserialized.sensors);
        assert_eq!(crew.gunnery, deserialized.gunnery);
    }
}
