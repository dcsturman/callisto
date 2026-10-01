use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skills {
  Pilot,
  EngineeringJump,
  EngineeringPower,
  EngineeringManeuver,
  Gunnery,
  Sensors,
  Leadership,
  Mechanic,
}

// Helper used by serde `skip_serializing_if` so that zero-valued integer
// crew skills are omitted from the JSON wire form. Keeps existing scenario
// JSON stable when new skills are added (e.g. `leadership`).
// Serde requires the predicate to take a reference, so suppress the
// pass-by-value lint that fires on this 1-byte type.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &u8) -> bool {
  *value == 0
}

/// One engineer on the crew, with the skills an engineer is rated in.
///
/// A ship can carry several, and each works their own job in a round, so they
/// are listed rather than flattened into one set of numbers.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Engineer {
  #[serde(default, skip_serializing_if = "is_zero")]
  pub jump: u8,
  #[serde(default, skip_serializing_if = "is_zero")]
  pub power: u8,
  #[serde(default, skip_serializing_if = "is_zero")]
  pub maneuver: u8,
  /// Repairs everything that is not a drive or the power plant: weapons,
  /// sensors and the bridge.
  #[serde(default, skip_serializing_if = "is_zero")]
  pub mechanic: u8,
  /// Recorded for completeness. Nothing in Callisto calls for it yet.
  #[serde(default, skip_serializing_if = "is_zero")]
  pub life_support: u8,
}

/// The crew aboard: who is at each station and how good they are.
///
/// Sensor operators and engineers are lists because a ship can carry several
/// of each and they are not interchangeable -- a second engineer is another
/// pair of hands with their own skills, not a bonus on the first one's.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "CrewWire", into = "CrewWire")]
pub struct Crew {
  pilot: u8,
  /// One entry per sensor operator on duty.
  sensors: Vec<u8>,
  /// One entry per engineer on duty.
  engineers: Vec<Engineer>,
  gunnery: Vec<u8>,
  /// Gunner (screen) skill, index-aligned with the ship's `screens` exactly as
  /// `gunnery` is with its weapons.  Out-of-range reads as 0, same as gunnery.
  screen_gunnery: Vec<u8>,
  leadership: u8,
}

/// The crew as it travels the wire and sits in a scenario file.
///
/// Separate from `Crew` so both shapes load: the single `sensors: 2` and
/// `engineering_jump: 3` that scenarios were written with before a ship could
/// carry more than one of each, and the lists written since. Everything is
/// written back out in the newer shape.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CrewWire {
  #[serde(default)]
  pilot: u8,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  sensors: Option<SensorsField>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  engineers: Option<Vec<Engineer>>,
  // The older flat engineering skills: read when `engineers` is absent, never
  // written.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  engineering_jump: Option<u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  engineering_power: Option<u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  engineering_maneuver: Option<u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  mechanic: Option<u8>,
  #[serde(default = "default_gunnery")]
  gunnery: Vec<u8>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  screen_gunnery: Vec<u8>,
  #[serde(default, skip_serializing_if = "is_zero")]
  leadership: u8,
}

/// `sensors` was one operator's skill and is now a list of them.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SensorsField {
  One(u8),
  Many(Vec<u8>),
}

impl From<CrewWire> for Crew {
  fn from(wire: CrewWire) -> Self {
    let sensors = match wire.sensors {
      None => vec![],
      Some(SensorsField::One(skill)) => vec![skill],
      Some(SensorsField::Many(skills)) => skills,
    };
    let engineers = wire.engineers.unwrap_or_else(|| {
      let legacy = Engineer {
        jump: wire.engineering_jump.unwrap_or(0),
        power: wire.engineering_power.unwrap_or(0),
        maneuver: wire.engineering_maneuver.unwrap_or(0),
        mechanic: wire.mechanic.unwrap_or(0),
        life_support: 0,
      };
      // A crew written before engineers were listed has one if it had any
      // engineering at all, and an empty engine room otherwise.
      if legacy == Engineer::default() {
        vec![]
      } else {
        vec![legacy]
      }
    });
    Crew {
      pilot: wire.pilot,
      sensors,
      engineers,
      gunnery: wire.gunnery,
      screen_gunnery: wire.screen_gunnery,
      leadership: wire.leadership,
    }
  }
}

impl From<Crew> for CrewWire {
  fn from(crew: Crew) -> Self {
    CrewWire {
      pilot: crew.pilot,
      sensors: Some(SensorsField::Many(crew.sensors)),
      engineers: Some(crew.engineers),
      engineering_jump: None,
      engineering_power: None,
      engineering_maneuver: None,
      mechanic: None,
      gunnery: crew.gunnery,
      screen_gunnery: crew.screen_gunnery,
      leadership: crew.leadership,
    }
  }
}

// Function just to provide a default value for gunnery deserialization
fn default_gunnery() -> Vec<u8> {
  vec![]
}

impl PartialEq for Crew {
  fn eq(&self, other: &Self) -> bool {
    self.pilot == other.pilot
      && self.sensors == other.sensors
      && self.engineers == other.engineers
      && self.gunnery == other.gunnery
      && self.screen_gunnery == other.screen_gunnery
      && self.leadership == other.leadership
  }
}

impl Crew {
  #[must_use]
  pub fn new() -> Crew {
    Crew {
      pilot: 0,
      sensors: vec![],
      engineers: vec![],
      gunnery: vec![],
      screen_gunnery: vec![],
      leadership: 0,
    }
  }

  /// Get the skill level for a crew for a particular skill.  Note that you cannot
  /// use this function to get gunnery skill as there may be multiple gunners in the crew. Use
  /// `get_gunnery` instead.
  ///
  /// For a station several people work, this is the best of them: the one who
  /// gets the job when nobody has said otherwise.
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
      Skills::EngineeringJump => self.get_engineering_jump(),
      Skills::EngineeringPower => self.get_engineering_power(),
      Skills::EngineeringManeuver => self.get_engineering_maneuver(),
      Skills::Sensors => self.get_sensors(),
      Skills::Leadership => self.leadership,
      Skills::Mechanic => self.get_mechanic(),
      Skills::Gunnery => panic!("(Crew.getSkill) Multiple gunners possible."),
    }
  }

  #[must_use]
  pub fn get_pilot(&self) -> u8 {
    self.pilot
  }

  /// How many sensor operators are on duty. Zero is a real answer: a ship with
  /// nobody sitting at the sensors.
  #[must_use]
  pub fn sensor_operators(&self) -> usize {
    self.sensors.len()
  }

  /// Every sensor operator's skill, in the order they were listed.
  #[must_use]
  pub fn sensor_skills(&self) -> &[u8] {
    &self.sensors
  }

  /// The skill of the operator at `index`, or the best aboard when the index
  /// names nobody -- an action that never said who was working it.
  #[must_use]
  pub fn get_sensors_at(&self, index: usize) -> u8 {
    self.sensors.get(index).copied().unwrap_or_else(|| self.get_sensors())
  }

  /// The best sensor operator aboard.
  #[must_use]
  pub fn get_sensors(&self) -> u8 {
    self.sensors.iter().copied().max().unwrap_or(0)
  }

  /// How many engineers are on duty.
  #[must_use]
  pub fn engineer_count(&self) -> usize {
    self.engineers.len()
  }

  #[must_use]
  pub fn engineers(&self) -> &[Engineer] {
    &self.engineers
  }

  /// The engineer at `index`, or the crew's best at each skill when the index
  /// names nobody.
  #[must_use]
  pub fn engineer_at(&self, index: usize) -> Engineer {
    self.engineers.get(index).cloned().unwrap_or_else(|| Engineer {
      jump: self.get_engineering_jump(),
      power: self.get_engineering_power(),
      maneuver: self.get_engineering_maneuver(),
      mechanic: self.get_mechanic(),
      life_support: 0,
    })
  }

  #[must_use]
  pub fn get_engineering_jump(&self) -> u8 {
    self.engineers.iter().map(|engineer| engineer.jump).max().unwrap_or(0)
  }

  #[must_use]
  pub fn get_engineering_power(&self) -> u8 {
    self.engineers.iter().map(|engineer| engineer.power).max().unwrap_or(0)
  }

  #[must_use]
  pub fn get_engineering_maneuver(&self) -> u8 {
    self.engineers.iter().map(|engineer| engineer.maneuver).max().unwrap_or(0)
  }

  #[must_use]
  pub fn get_mechanic(&self) -> u8 {
    self.engineers.iter().map(|engineer| engineer.mechanic).max().unwrap_or(0)
  }

  #[must_use]
  pub fn get_leadership(&self) -> u8 {
    self.leadership
  }

  #[must_use]
  pub fn get_gunnery(&self, gun: usize) -> u8 {
    if gun >= self.gunnery.len() {
      return 0;
    }
    self.gunnery[gun]
  }

  /// Whether anyone is actually on this mount.
  ///
  /// Distinct from a skill of 0, the way it is for screens: an unmanned
  /// mount has nobody at it, which is what a Virtual Gunner stands in for.
  #[must_use]
  pub fn has_gunner(&self, gun: usize) -> bool {
    gun < self.gunnery.len()
  }

  /// Gunner (screen) skill for the screen at `screen`, or 0 if unspecified.
  #[must_use]
  pub fn get_screen_gunnery(&self, screen: usize) -> u8 {
    if screen >= self.screen_gunnery.len() {
      return 0;
    }
    self.screen_gunnery[screen]
  }

  /// Whether anyone is actually on this screen.
  ///
  /// Distinct from a skill of 0: an unstaffed screen has nobody to take the
  /// Angle Screens reaction at all, while a green rating 0 gunner can.
  #[must_use]
  pub fn has_screen_gunner(&self, screen: usize) -> bool {
    screen < self.screen_gunnery.len()
  }

  /// Append a Gunner (screen) skill, mirroring `add_gunnery`.
  pub fn add_screen_gunnery(&mut self, skill: u8) {
    self.screen_gunnery.push(skill);
  }

  /// Sets a crew skill level.  Note that setting a skill this way for gunnery is not allowed.
  /// Instead use `add_gunnery`.
  ///
  /// A station several people work sets the first of them, taking somebody on
  /// if the ship had nobody there.
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
      Skills::EngineeringJump => self.first_engineer().jump = value,
      Skills::EngineeringPower => self.first_engineer().power = value,
      Skills::EngineeringManeuver => self.first_engineer().maneuver = value,
      Skills::Mechanic => self.first_engineer().mechanic = value,
      Skills::Sensors => {
        if self.sensors.is_empty() {
          self.sensors.push(value);
        } else {
          self.sensors[0] = value;
        }
      }
      Skills::Leadership => self.leadership = value,
      Skills::Gunnery => panic!("Cannot use set_skill for gunnery. Use add_gunnery instead."),
    }
  }

  /// The first engineer, taking one on if the engine room was empty.
  fn first_engineer(&mut self) -> &mut Engineer {
    if self.engineers.is_empty() {
      self.engineers.push(Engineer::default());
    }
    &mut self.engineers[0]
  }

  /// Add a sensor operator to the watch.
  pub fn add_sensor_operator(&mut self, skill: u8) {
    self.sensors.push(skill);
  }

  /// Add an engineer to the watch.
  pub fn add_engineer(&mut self, engineer: Engineer) {
    self.engineers.push(engineer);
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

  /// A crew set through the public API, which is how every caller builds one.
  fn skilled() -> Crew {
    let mut crew = Crew::new();
    crew.set_skill(Skills::Pilot, 3);
    crew.set_skill(Skills::EngineeringJump, 2);
    crew.set_skill(Skills::EngineeringPower, 1);
    crew.set_skill(Skills::EngineeringManeuver, 4);
    crew.set_skill(Skills::Sensors, 5);
    crew.set_skill(Skills::Leadership, 6);
    crew.set_skill(Skills::Mechanic, 7);
    crew
  }

  #[test_log::test]
  fn test_crew_new() {
    let crew = Crew::new();
    assert_eq!(crew.get_pilot(), 0);
    assert_eq!(crew.get_engineering_jump(), 0);
    assert_eq!(crew.get_sensors(), 0);
    assert_eq!(crew.get_leadership(), 0);
    assert_eq!(crew.sensor_operators(), 0, "nobody at the sensors");
    assert_eq!(crew.engineer_count(), 0, "and nobody in the engine room");
    assert_eq!(crew.gunnery, Vec::<u8>::new());
  }

  #[test_log::test]
  fn test_get_skill() {
    let crew = skilled();
    assert_eq!(crew.get_skill(Skills::Pilot), 3);
    assert_eq!(crew.get_skill(Skills::EngineeringJump), 2);
    assert_eq!(crew.get_skill(Skills::EngineeringPower), 1);
    assert_eq!(crew.get_skill(Skills::EngineeringManeuver), 4);
    assert_eq!(crew.get_skill(Skills::Sensors), 5);
    assert_eq!(crew.get_skill(Skills::Leadership), 6);
    assert_eq!(crew.get_skill(Skills::Mechanic), 7);
  }

  #[test_log::test]
  #[should_panic(expected = "(Crew.getSkill) Multiple gunners possible.")]
  fn test_get_skill_gunnery_panic() {
    let crew = Crew::new();
    let _ = crew.get_skill(Skills::Gunnery);
  }

  #[test_log::test]
  fn test_get_individual_skills() {
    let crew = skilled();
    assert_eq!(crew.get_pilot(), 3);
    assert_eq!(crew.get_engineering_jump(), 2);
    assert_eq!(crew.get_engineering_power(), 1);
    assert_eq!(crew.get_engineering_maneuver(), 4);
    assert_eq!(crew.get_sensors(), 5);
    assert_eq!(crew.get_leadership(), 6);
    assert_eq!(crew.get_mechanic(), 7);
  }

  #[test_log::test]
  fn test_get_gunnery() {
    let mut crew = Crew::new();
    crew.add_gunnery(1);
    crew.add_gunnery(2);
    crew.add_gunnery(3);

    assert_eq!(crew.get_gunnery(0), 1);
    assert_eq!(crew.get_gunnery(1), 2);
    assert_eq!(crew.get_gunnery(2), 3);
    assert_eq!(crew.get_gunnery(3), 0); // Out of range
  }

  #[test_log::test]
  fn test_set_skill() {
    let crew = skilled();
    assert_eq!(crew.get_pilot(), 3);
    assert_eq!(crew.engineer_count(), 1, "setting engineering takes on one engineer");
    assert_eq!(crew.sensor_operators(), 1, "and setting sensors puts one on the station");
  }

  /// Several operators are several people, not one better one.
  #[test_log::test]
  fn a_watch_of_several_operators_is_addressed_one_at_a_time() {
    let mut crew = Crew::new();
    crew.add_sensor_operator(1);
    crew.add_sensor_operator(4);

    assert_eq!(crew.sensor_operators(), 2);
    assert_eq!(crew.get_sensors_at(0), 1);
    assert_eq!(crew.get_sensors_at(1), 4);
    assert_eq!(crew.get_sensors(), 4, "the best of them, when nobody is named");
    assert_eq!(crew.get_sensors_at(7), 4, "and for an index nobody is at");
  }

  #[test_log::test]
  fn engineers_are_addressed_one_at_a_time_too() {
    let mut crew = Crew::new();
    crew.add_engineer(Engineer {
      jump: 3,
      power: 0,
      maneuver: 1,
      mechanic: 0,
      life_support: 0,
    });
    crew.add_engineer(Engineer {
      jump: 0,
      power: 2,
      maneuver: 0,
      mechanic: 4,
      life_support: 1,
    });

    assert_eq!(crew.engineer_count(), 2);
    assert_eq!(crew.engineer_at(0).jump, 3);
    assert_eq!(crew.engineer_at(1).mechanic, 4);
    // The crew's best at each skill, which is who gets a job nobody was named for.
    assert_eq!(crew.get_engineering_jump(), 3);
    assert_eq!(crew.get_mechanic(), 4);
    assert_eq!(crew.engineer_at(9).jump, 3, "an index nobody is at falls back to the best");
  }

  /// Crews written before a ship could carry more than one of either still
  /// load, and come back out in the newer shape.
  #[test_log::test]
  fn the_older_flat_crew_still_loads() {
    let legacy = r#"{"pilot":3,"engineering_jump":2,"engineering_power":1,"engineering_maneuver":4,
                     "sensors":5,"gunnery":[2,1],"leadership":6,"mechanic":7}"#;
    let crew: Crew = serde_json::from_str(legacy).expect("a crew written the old way should load");

    assert_eq!(crew.get_pilot(), 3);
    assert_eq!(crew.sensor_operators(), 1);
    assert_eq!(crew.get_sensors(), 5);
    assert_eq!(crew.engineer_count(), 1, "the old flat skills are one engineer");
    assert_eq!(crew.get_engineering_jump(), 2);
    assert_eq!(crew.get_mechanic(), 7);

    let json = serde_json::to_value(&crew).expect("and should write back out");
    assert_eq!(json["sensors"], serde_json::json!([5]));
    assert_eq!(json["engineers"][0]["jump"], 2);
    assert!(json.get("engineering_jump").is_none(), "written in the newer shape only");
  }

  #[test_log::test]
  fn a_listed_crew_loads_as_listed() {
    let listed = r#"{"pilot":1,"sensors":[2,3],
                     "engineers":[{"jump":1},{"mechanic":4,"life_support":2}],"gunnery":[]}"#;
    let crew: Crew = serde_json::from_str(listed).expect("a listed crew should load");

    assert_eq!(crew.sensor_operators(), 2);
    assert_eq!(crew.engineer_count(), 2);
    assert_eq!(crew.engineer_at(1).mechanic, 4);
    assert_eq!(crew.engineer_at(1).life_support, 2);
  }

  #[test_log::test]
  fn an_empty_crew_has_nobody_aboard() {
    let crew: Crew = serde_json::from_str(r#"{"pilot":0,"gunnery":[]}"#).expect("an empty crew should load");
    assert_eq!(crew.sensor_operators(), 0);
    assert_eq!(crew.engineer_count(), 0);
  }

  #[test_log::test]
  fn test_default_gunnery() {
    let default_gunnery = default_gunnery();
    assert_eq!(default_gunnery, Vec::<u8>::new());
  }

  #[test_log::test]
  fn test_crew_serialization_deserialization() {
    let mut crew = skilled();
    crew.add_sensor_operator(2);
    crew.add_engineer(Engineer {
      power: 3,
      ..Engineer::default()
    });
    crew.add_gunnery(1);
    crew.add_gunnery(2);

    let serialized = serde_json::to_string(&crew).unwrap();
    let deserialized: Crew = serde_json::from_str(&serialized).unwrap();

    assert_eq!(crew, deserialized, "a crew should survive a round trip unchanged");
  }
}
