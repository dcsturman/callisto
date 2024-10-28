use crate::combat::{HdEntry, HdEntryTable, ShipSystem};

pub const EXTERNAL_DAMAGE_TABLE: [ShipSystem; 13] = [
    ShipSystem::Hull,
    ShipSystem::Hull,
    ShipSystem::Hull,
    ShipSystem::Sensors,
    ShipSystem::Manuever,
    ShipSystem::Turret,
    ShipSystem::Hull,
    ShipSystem::Armor,
    ShipSystem::Hull,
    ShipSystem::Fuel,
    ShipSystem::Manuever,
    ShipSystem::Sensors,
    ShipSystem::Hull,
];

pub const INTERNAL_DAMAGE_TABLE: [ShipSystem; 13] = [
    ShipSystem::Hull,
    ShipSystem::Hull,
    ShipSystem::Structure,
    ShipSystem::Powerplant,
    ShipSystem::Jump,
    ShipSystem::Turret,
    ShipSystem::Structure,
    ShipSystem::Structure,
    ShipSystem::Structure,
    ShipSystem::Hold,
    ShipSystem::Jump,
    ShipSystem::Powerplant,
    ShipSystem::Bridge,
];

pub const HIT_DAMAGE_TABLE: HdEntryTable = [
    HdEntry {
        top_range: 0,
        single_hits: 0,
        double_hits: 0,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 4,
        single_hits: 1,
        double_hits: 0,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 8,
        single_hits: 2,
        double_hits: 0,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 12,
        single_hits: 0,
        double_hits: 1,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 16,
        single_hits: 3,
        double_hits: 0,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 20,
        single_hits: 2,
        double_hits: 1,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 24,
        single_hits: 0,
        double_hits: 2,
        triple_hits: 0,
    },
    HdEntry {
        top_range: 28,
        single_hits: 0,
        double_hits: 0,
        triple_hits: 1,
    },
    HdEntry {
        top_range: 32,
        single_hits: 1,
        double_hits: 0,
        triple_hits: 1,
    },
    HdEntry {
        top_range: 36,
        single_hits: 0,
        double_hits: 1,
        triple_hits: 1,
    },
    HdEntry {
        top_range: 40,
        single_hits: 1,
        double_hits: 1,
        triple_hits: 1,
    },
    HdEntry {
        top_range: 44,
        single_hits: 0,
        double_hits: 0,
        triple_hits: 2,
    },
];
