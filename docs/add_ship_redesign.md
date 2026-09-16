# Add Ship redesign: filtered design picker + per-ship hardpoint armament

Status: **design only, nothing implemented.**
Date: 2026-09-05.
Scope: `fe/callisto/src/components/controls/AddShip.tsx` and everything downstream of a
ship's weapon list.

---

## 1. Current state

### 1.1 The dialog

`fe/callisto/src/components/controls/AddShip.tsx` is a single `<form>` inside an
`Accordion`:

| Element | Location |
| --- | --- |
| Name / Position / Velocity inputs | `AddShip.tsx:154-218` |
| `<ShipDesignList>` — the design picker | `AddShip.tsx:219-223`, defined at `AddShip.tsx:305-378` |
| `<CrewBuilder>` | `AddShip.tsx:226-231` |
| Submit (`Add`/`Update`) | `AddShip.tsx:232-236` |

The design picker is one bare `<select>` (`AddShip.tsx:340-364`). Its options are
`Object.values(args.shipDesigns)` sorted by `displacement`, then `name`
(`AddShip.tsx:350-357`), each rendered as `` `${design.name} (${design.displacement})` ``
(`AddShip.tsx:362`). There is no filtering, no grouping, and no search.

A `react-tooltip` (`AddShip.tsx:365-369`) renders `ShipDesignDetails`
(`AddShip.tsx:242-303`), which prints displacement/hull/armor/power/thrust/jump and a
prose weapon list built from `compressedWeaponsFromTemplate`.

**Verified count: 79 design files** in `callisto/ship_templates/` as of this writing. A
flat 79-entry `<select>` is the problem being fixed.

### 1.2 Where the design list comes from

- Backend serves `ResponseMsg::DesignTemplateResponse(HashMap<String, ShipDesignTemplate>)`
  (`callisto/src/payloads.rs:222`, `:381`), built by `PlayerManager::get_designs`
  (`callisto/src/player.rs:311-321`), sent on join (`processor.rs:384`), on request
  (`processor.rs:867-869`) and on template reload (`processor.rs:894`).
- Frontend parses it at `fe/callisto/src/lib/serverManager.ts:189-190` and stores it in
  `state.server.templates` (`fe/callisto/src/state/serverSlice.ts:27`, `:62`,
  selector at `:84`).

### 1.3 The add-ship request today

`fe/callisto/src/lib/serverManager.ts:312-324`:

```js
export function addShip(ship: Ship) {
  const payload = { AddShip: {
    name: ship.name, position: ship.position, velocity: ship.velocity,
    design: ship.design, crew: ship.crew } };
  socket.send(JSON.stringify(payload));
}
```

Backend: `AddShipMsg` (`callisto/src/payloads.rs:66-75`) → `RequestMsg::AddShip`
(`payloads.rs:348`) → `processor.rs:621` → `PlayerManager::add_ship`
(`callisto/src/player.rs:220-240`) → `Entities::add_ship`
(`callisto/src/entity.rs:352-368`) → `Ship::new` (`callisto/src/ship.rs:407-443`).

**A ship never carries a weapon list.** It carries a design *name*
(`Ship.design: Arc<ShipDesignTemplate>` serialized as a bare string via
`TemplateNameOnly`, `ship.rs:152-155` and `ship.rs:869-879`), and every weapon lookup
dereferences `self.design.weapons`.

### 1.4 Do bays render in the frontend today? **Yes.**

`fe/callisto/src/lib/weapon.ts:13-29`, `weaponToString`:

- `Barbette` is a Rust unit variant → JSON string `"Barbette"` → caught by
  `typeof weapon.mount === "string"` → `"<kind> Barbette"`.
- `Turret(n)` → `{"Turret": n}` → `"Single/Double/Triple <kind> Turret"`.
- `Bay(size)` → `{"Bay": "Small"}` → caught by `"Bay" in weapon.mount` →
  `"Small <kind> Bay"`. **This works at runtime.**

But the declared type is wrong: `WeaponMount = string | {Turret: number} | {BaySize: string}`
(`weapon.ts:1`) uses the key `BaySize`, not `Bay`. The code still compiles under the
project's TypeScript 4.9 (`npx tsc --noEmit` passes, verified) only because TS 4.9's `in`
narrowing widens the unlisted `Bay` property to `unknown`, and `unknown` is legal in a
template literal. So bays display correctly but are entirely untyped. **Fix
`weapon.ts:1` to `{Bay: "Small" | "Medium" | "Large"}` as part of this work** — the
hardpoint editor needs real types to build mount values.

Second gap: `fe/callisto/src/lib/shipDesignTemplates.ts:3-19` — the `ShipDesignTemplate`
interface does **not** yet have `role` or `source`, even though the Rust struct does
(`callisto/src/ship.rs:316`, `:319`). Must be added (both optional, `?:`) before any
role filter can be written.

### 1.5 Weapon indexing today

`weapon_id` is a dense index into `design.weapons`. It appears in the wire protocol in
`ShipAction::FireAction`/`PointDefenseAction`/`DeleteFireAction`
(`callisto/src/action.rs:190`, `:200`, `:203`) and in `BoostTarget`
(`action.rs:21-22`). `crew.gunnery` is a `Vec<u8>` indexed by the same number
(`callisto/src/crew.rs:37`, `get_gunnery` at `crew.rs:114-119` — returns 0 when the index
is out of range, so a short or long array is harmless). Real scenario data already has
oversized arrays: `callisto/scenarios/Marduk Encounter.json` gives *HMS Executor* a
21-entry `gunnery` array for a 3-weapon design.

---

## 2. Data reality check

Computed over all 79 files in `callisto/ship_templates/`.

### 2.1 `role` / `source` coverage

| Field | Set | Unset |
| --- | --- | --- |
| `role` | 25 | **54** |
| `source` | 25 (14 "Ships of the Reach (Aslan)", 11 "Ships of the Reach") | 54 |

> **Correction to the brief:** it is 54 designs without a `role`, not ~19. The
> unclassified bucket is the *majority* of the library, which changes the design: the
> role filter cannot be the primary navigation aid on its own.

Role values present, with counts:

```
Trader 4   Scout 3   Carrier 2   Courier 2   Support 2   Utility 2
Transport 1  Raider 1  Cruiser 1  Escort 1  Gunship 1  Liner 1
Warship 1  Research 1  Prospector 1  Slaver 1
```

16 distinct roles, 10 of which have exactly one design. `role` is `Option<String>` on the
Rust side — free-form. The UI must derive the option list from the data, never hardcode it.

### 2.2 Hardpoint and Firmpoint rules (corrected)

**An earlier draft of this document was wrong on two counts** — it claimed bays do not
consume hardpoints, and it assumed sub-100-ton craft simply get `floor(tons/100) = 0`.
Both are corrected below from High Guard pp. 26 and 31.

**Ships of 100 tons or more** get **one Hardpoint per 100 tons of hull**, and each weapon
system consumes hardpoints per High Guard p.26:

| Weapon System | Hardpoints used |
| --- | ---: |
| Fixed Mount | 1 |
| Turret (single/double/triple) | 1 |
| Barbette | 1 |
| Small Bay | 1 |
| Medium Bay | 1 |
| **Large Bay** | **5** |
| Spinal Mount | weapon tonnage / 100 |

Note a turret costs 1 hardpoint regardless of whether it is single, double or triple — the
turret is the hardpoint, not the guns in it. This is why regrouping mixed turrets while
preserving *mount* count (the policy adopted for these designs) also preserves hardpoint
usage, whereas splitting them into more mounts silently overruns the allowance.

**Ships under 100 tons** have **Firmpoints** instead (High Guard p.26):

| Hull | Firmpoints |
| --- | ---: |
| under 35 tons | 1 |
| 35-69 tons | 2 |
| 70-99 tons | 3 |

- A Firmpoint holds **only one weapon**.
- **One (and only one)** Firmpoint may be upgraded to a **single** turret — not double or
  triple — which may fire in all directions as normal.
- A Barbette consumes **three** Firmpoints.

### 2.3 Actual compliance across the library

Recomputed over all 79 designs with the rules above. **4 designs flag, and 2 of those are
artifacts of our schema rather than bad data:**

| Design | Tons | Allowance | Used | Status |
| --- | ---: | --- | ---: | --- |
| `heavy_fighter` | 50 | 2 firmpoints | 2 | **Legal.** Book: *Single Turret (beam laser)* + *Fixed Mount (missile rack)*. |
| `troop_transport` | 50 | 2 firmpoints | 2 | **Legal.** Book: *Single Turret (sandcaster)* + *Fixed Mount (missile rack)*. |
| `indigo_pirate_carrier` | 300 | 3 hardpoints | 6 -> 3 | **Was a real overrun**, since fixed: regrouped to 2x Beam T3 + 1x Missile T3, conserving 6 beam + 3 missile. |
| `excelsior` | 200 | 2 hardpoints | 3 | **Legal in intent; a schema artifact.** It is a particle barbette plus *one triple turret holding 2 missile racks and a sandcaster* - 2 hardpoints. `WeaponMount` cannot express a mixed turret, so it is stored as `Missile T2` + `Sand T1` and counts as 3. Left as-is by choice. |

**Every one of the four flags is a schema gap, not bad data.** With the rules applied
correctly and `indigo_pirate_carrier` regrouped, **no design in the library actually breaks
the hardpoint or firmpoint rules.** Two gaps produce all the false positives:

**Gap 1: mixed turrets.** A turret may hold different weapon types (Core Rulebook, "Double
and Triple Turrets"), but `Weapon` has a single `kind`, so a mixed turret must be split into
one mount per type. That inflates the mount count and therefore the apparent hardpoint use -
which is exactly what happens to `excelsior`. Supporting mixed turrets would also remove the
need for the regrouping policy applied to the book designs, which currently loses or gains a
gun in the cases that do not divide evenly.

**Gap 2: fixed mounts.** `WeaponMount` has no `FixedMount`; fixed mounts are
currently encoded as `Turret(1)`, indistinguishable from a genuine single turret. That is
harmless on large hulls where both cost 1 hardpoint, but on small craft it matters twice
over: only one Firmpoint may be a turret, and a fixed mount is direction-limited while a
turret is not. It is why `heavy_fighter` and `troop_transport` above look illegal when they
are not. Designs currently affected: `ultralight_fighter`, `light_fighter`,
`military_gig_close_escort_variant`, `heavy_fighter`, `troop_transport`,
`merchant_cruiser_leviathan`.

**Recommendation:** add `WeaponMount::FixedMount` before building the hardpoint editor -
the UI needs it as a distinct dropdown option regardless, and without it the editor cannot
enforce "at most one turret" on small craft. Mixed-turret support is the larger change and
can follow; until it lands, the editor should count hardpoints by *distinct turret*, not by
mount, or designs like `excelsior` will read as over-allowance.

### 2.4 Hardpoint allowance formula

```
capacity(design) =
    displacement >= 100 -> ("hardpoints", displacement / 100)
    displacement <  35  -> ("firmpoints", 1)
    displacement <  70  -> ("firmpoints", 2)
    otherwise           -> ("firmpoints", 3)

cost(mount) =
    Bay(Large)                  -> 5
    Barbette on a small craft   -> 3
    anything else               -> 1
```

With the corrected rules the "never shrink below the design's existing weapons" clause
proposed in the earlier draft is **no longer needed for compatibility** — once
`indigo_pirate_carrier` is regrouped, every book design fits its allowance. Only
`excelsior`, a custom design, exceeds it. That is a much better place to be: the editor can
enforce the real rule rather than grandfathering violations.

### 2.5 Deferred Firmpoint rules (TODO, not implemented)

Recorded so they are not lost. None of these are modelled yet:

- Firmpoints other than the single upgraded turret fire only along the thrust vector.
- A weapon on a Firmpoint has Medium range or less reduced to **Close**, and its range may
  not be increased beyond Close by any means.
- Those range limits do **not** apply to missiles or torpedoes.
- Power requirements for a weapon on a Firmpoint are reduced by 25% (rounding up).
- Torpedoes are not implemented at all (currently mapped to missiles).

## 3. Proposed UI

### 3.1 Design selection

Three controls replacing the single `<select>`:

1. **Role** `<select>` — `All` (default), then every distinct non-null `role` sorted
   alphabetically, then `Unclassified` pinned last. Derived from the template data, not
   hardcoded. `Unclassified` selects the 54 designs with `role == null`; **`All` always
   includes them**, so no design can vanish from the picker.
2. **Filter** text input — case-insensitive substring match against `name`, `role` and
   `source`. With a 79-entry library and a 16-way role split whose long tail is
   single-design roles, typing three letters beats any dropdown. This is the control that
   actually solves the stated problem; the role dropdown is complementary.
3. **Design** `<select>` — the existing control, filtered by (1) and (2), still sorted by
   displacement then name. When role is `All`, wrap options in `<optgroup label={role}>`
   so the flat list acquires structure for free.

**`source` does not get its own dropdown.** Three distinct values covering 25 of 79
designs is not enough signal to spend a control on, and it correlates almost perfectly
with role coverage (the same 25 designs carry both). Instead: show it in the option label
when present — `Sakhai Assault Carrier (2000) · Ships of the Reach (Aslan)` — and in the
existing hover tooltip. It remains reachable via the text filter. Adding a real `source`
dropdown later is a five-line change if the user disagrees.

### 3.2 Hardpoints

A new section between the design picker and `CrewBuilder`. One row per hardpoint:

```
#   Mount                Weapon
1   [Triple Turret  v]   [Beam      v]
```

- Mount options: `None`, `Single Turret`, `Double Turret`, `Triple Turret`, `Barbette`,
  `Small Bay`, `Medium Bay`, `Large Bay`.
- **Turret sizes are restricted to 1/2/3 — this is a hard constraint, not a style choice.**
  `impl From<&Weapon> for String` at `callisto/src/ship.rs:1119-1136` `panic!`s on any
  other turret size (`ship.rs:1125-1127`), and that conversion runs inside combat
  resolution (`combat.rs:510`). A client that sent `Turret(4)` would crash the server on
  the first weapon critical hit. See §6 — the server must validate this too.
- Weapon `<select>` (`Beam`/`Pulse`/`Missile`/`Sand`/`Particle`) is rendered only when
  mount ≠ `None`.
- Rows are pre-populated from `design.weapons` in order, and **re-populated whenever the
  design changes** (a `useEffect` on `addShipData.design`; see §3.4 for the edit-collision
  question).

### 3.3 Mockup

```
┌─ Add Ship ─────────────────────────────────────────────────────┐
│ Name      [ Rhylanor Runner                                  ] │
│ Position  [ 0        ][ 0        ][ 0        ]  (km)           │
│ Velocity  [ 0        ][ 0        ][ 0        ]  (m/s)          │
│                                                                │
│ Role      [ All                                            v ] │
│ Filter    [ trad                                    ] (4/79)   │
│ Design ⓘ  [ Free Trader (200)                              v ] │
│           ┌──────────────────────────────────────────────────┐ │
│           │ ── Trader ───────────────────────────────────    │ │
│           │   Fast Trader Type A3 (200) · High Guard         │ │
│           │   Light Trader Aoa Iw (300) · Ships of the Reach │ │
│           │   Trader Eakhau (400) · Ships of the Reach (Asl… │ │
│           │ ── Unclassified ─────────────────────────────    │ │
│           │   Far Trader (200)                              │ │
│           │   Free Trader (200)                             │ │
│           │   Stretched Trader (400)                        │ │
│           └──────────────────────────────────────────────────┘ │
│ ────────────────────────────────────────────────────────────── │
│ Hardpoints                              2 of 2 used            │
│                                                                │
│  #   Mount                    Weapon                           │
│  1   [ Double Turret     v ]  [ Pulse           v ]            │
│  2   [ Double Turret     v ]  [ Sand            v ]            │
│ ────────────────────────────────────────────────────────────── │
│ Crew                                                           │
│  Pilot [2]  Engineering [2/2/2]  Sensors [4]  Leadership [3]   │
│  Gunner 1 [2]   Gunner 2 [0]                                   │
│ ────────────────────────────────────────────────────────────── │
│                        [        Add        ]                   │
└────────────────────────────────────────────────────────────────┘
```

Over-allowance case (`Indigo Pirate Carrier`, 300 t, 6 weapons, allowance 3):

```
│ Hardpoints                              6 of 3 used  ⚠         │
│  #   Mount                    Weapon                           │
│  1   [ Triple Turret     v ]  [ Beam            v ]            │
│  2   [ Triple Turret     v ]  [ Beam            v ]            │
│  3   [ Triple Turret     v ]  [ Missile         v ]            │
│  4 ⚠ [ Triple Turret     v ]  [ Missile         v ]            │
│  5 ⚠ [ Triple Turret     v ]  [ Sand            v ]            │
│  6 ⚠ [ Triple Turret     v ]  [ Sand            v ]            │
│      ⚠ rows 4-6 exceed the 1-per-100-tons allowance            │
│        (inherited from the design; may be cleared, not added)  │
```

Small craft (`Ultralight Fighter`, 6 t, 1 weapon):

```
│ Hardpoints                              1 of 1 used  ⚠         │
│  1 ⚠ [ Single Turret     v ]  [ Pulse           v ]            │
```

### 3.4 Frontend state and behaviour

`addShipData` (`AddShip.tsx:49`) gains `weapons: (Weapon | null)[]`, one entry per
hardpoint row, `null` = mount `None`.

- On mount and on design change: `weapons = padToLength(design.weapons, hardpoints(design))`.
- On submit: compact to a dense list, `weapons.filter(w => w !== null)`, preserving order.
  **This keeps `weapon_id` semantics unchanged** — it stays a dense 0-based index into the
  ship's weapon list, exactly as `FireAction`/`PointDefenseAction`/`BoostTarget` already
  assume (`action.rs:190-203`, `:21-22`). Hardpoint numbers are a UI concept only and are
  never sent over the wire.
- `CrewBuilder` currently derives gunner count from `shipDesign.weapons.length`
  (`CrewBuilder.tsx:46`). It must take the compacted per-ship weapon count instead, so the
  gunner rows track the actual armament.
- Editing an existing ship (the "Update" path, `AddShip.tsx:51-68`, `:78-95`) must load
  `ship.weapons` when present rather than re-deriving from the design.

---

## 4. Backend changes

### 4.1 The model change

Add to `Ship` (`callisto/src/ship.rs:139-...`, alongside `design` at `:152-155`):

```rust
  /// Per-ship armament. `None` means "inherit the design's weapons" — this is
  /// what every pre-existing scenario deserializes to, and what the server
  /// stores when a client omits the field.
  #[serde(default)]
  pub weapons: Option<Vec<Weapon>>,
```

`Ship` already carries `#[skip_serializing_none]` (`ship.rs:135`), so `None` is omitted on
the wire and existing scenario JSON is byte-identical.

Add one accessor and route **every** weapon read through it:

```rust
  #[must_use]
  pub fn weapons(&self) -> &[Weapon] {
    self.weapons.as_deref().unwrap_or(&self.design.weapons)
  }
```

`Ship::get_weapon` (`ship.rs:553-555`) becomes `&self.weapons()[weapon_id]`.

### 4.2 Every site that assumes `ship.design.weapons`

Grep: `rg '\.design\.weapons|get_weapon\(' callisto/src`.

| File:line | What it does | Change |
| --- | --- | --- |
| `ship.rs:312` | `ShipDesignTemplate.weapons` field | unchanged — designs keep their default armament |
| `ship.rs:426` | `Ship::new` sets `active_weapons: vec![true; design.weapons.len()]` | must size from the *ship's* weapons; `Ship::new` gains a `weapons: Option<Vec<Weapon>>` param |
| `ship.rs:455` | `fixup_current_values` resets `active_weapons` from `self.design.weapons.len()` | → `self.weapons().len()` |
| `ship.rs:553-554` | `get_weapon` | → `self.weapons()` |
| `combat.rs:510` | weapon-crit message: `String::from(&defender.design.weapons[selected_index])` | → `defender.weapons()`. **Borrow-check note:** line 511 mutates `defender.active_weapons` before this read; with an accessor returning a borrow of `self`, hoist the `String::from(...)` above the mutation or bind it first |
| `combat.rs:798` | `attacker.get_weapon(*weapon_id)` | free — already goes through the accessor |
| `combat.rs:1005-1035` | `create_sand_counts` iterates `ship.design.weapons` zipped with `active_weapons` and `crew.get_gunnery(index)` | → `ship.weapons()` |
| `combat.rs:1060-1075` | `build_point_defense_tallies` — same pattern | → `ship.weapons()` |
| `action.rs:308-316` | `DeleteFireAction` uses `let current_template = &ship.design;` then `current_template.weapons[weapon_id]` to find equal weapons | → bind `let weapons = ship.weapons();` and index that |
| `entity.rs:352-368` | `Entities::add_ship` | signature gains `weapons: Option<Vec<Weapon>>`; the existing-ship branch (`entity.rs:356-362`) must set `ship.weapons` before `fixup_current_values()` |
| `player.rs:220-240` | `PlayerManager::add_ship` | pass `ship.weapons` through |
| `payloads.rs:66-75` | `AddShipMsg` | gains the field — see §5 |

Also update the two `ShipDesignTemplate` literals used by tests (`ship.rs:1380`,
`ship.rs:2149`) only if their `Ship` constructions change shape.

### 4.3 Scenario save/load round-trip

Nothing to do beyond the `#[serde(default)]`.

- Scenario files store ships as flat serialized `Ship`s with `design` as a name string
  (verified in `callisto/scenarios/Marduk Encounter.json`). Old files have no `weapons`
  key → `None` → design armament. Identical behaviour.
- Saving a scenario re-serializes `Ship`; ships with per-ship armament emit a `weapons`
  array, ships without emit nothing (`skip_serializing_none`).
- **Reset works for free.** `Server::reset` (`server.rs:113-115`) assigns from
  `initial_scenario`, and `Entities::deep_copy_into` (`entity.rs:166-176`) uses the derived
  `ship.clone()`, which copies any new field automatically.
- `Entities::validate` (`entity.rs:1137-...`) only checks planet-primary and missile-target
  pointers. No change.

### 4.4 `crew.gunnery` indexing

`Crew.gunnery: Vec<u8>` (`crew.rs:37`) is positional per weapon, read via
`get_gunnery(gun)` (`crew.rs:114-119`) which **returns 0 for out-of-range indices**. There
is no length invariant to maintain, and real data already violates any such invariant
(the 21-entry array in `Marduk Encounter.json` for a 3-weapon ship). So:

- **No schema change to `Crew` is required.**
- Consequence to state plainly in the UI: gunnery skill is bound to the *slot*, not to the
  weapon. If a user changes hardpoint 2 from a Beam turret to a Missile turret, the gunner
  assigned to slot 2 keeps their skill. That is the desired behaviour for a referee tool.
- Because the wire list is the *compacted* list (§3.4), setting hardpoint 1 to `None`
  shifts every later weapon's `weapon_id` down by one and therefore re-associates gunnery
  entries. This is only observable while the dialog is open (queued `ShipAction`s for a
  ship being edited are a separate concern — see §7).

### 4.5 Places that need *no* change

`entity.rs:753-761` (missile TL), `entity.rs:952-954` (stealth), `entity.rs:1011-1105`
(countermeasures), `entity.rs:1372` (jump fuel), `ship.rs:447-454` (`fixup_current_values`
stat clamps), `ship.rs:505` (`best_thrust`), `combat.rs:414-452` (power/fuel crits) all
read non-weapon design fields.

Notably: **the engine never validates weapon legality against tonnage, power or cost.**
There is no `hardpoint` concept anywhere in `callisto/src` (grepped). So per-ship armament
introduces no rules-consistency regression — the engine was already unvalidated. See §7.

---

## 5. Wire-protocol change

`callisto/src/payloads.rs:66-75` becomes:

```rust
#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct AddShipMsg {
  pub name: String,
  #[serde_as(as = "Vec3asVec")]
  pub position: Vec3,
  #[serde_as(as = "Vec3asVec")]
  pub velocity: Vec3,
  pub design: String,
  pub crew: Option<Crew>,
  /// Per-ship armament. Absent or null = inherit the design's weapons.
  pub weapons: Option<Vec<Weapon>>,
}
```

The struct already has `#[skip_serializing_none]`, so `weapons` is omitted when `None`
and old clients that never send it keep working unchanged.

Exact new request shape:

```json
{
  "AddShip": {
    "name": "Rhylanor Runner",
    "position": [0.0, 0.0, 0.0],
    "velocity": [0.0, 0.0, 0.0],
    "design": "Free Trader",
    "crew": { "pilot": 2, "engineering_jump": 0, "engineering_power": 0,
              "engineering_maneuver": 0, "sensors": 1,
              "gunnery": [2, 0], "leadership": 0 },
    "weapons": [
      { "kind": "Pulse",    "mount": { "Turret": 2 } },
      { "kind": "Missile",  "mount": { "Turret": 3 } },
      { "kind": "Particle", "mount": "Barbette" },
      { "kind": "Missile",  "mount": { "Bay": "Small" } }
    ]
  }
}
```

`Weapon` serialization is unchanged and already externally-tagged the way the frontend
expects: `Turret(u8)` → `{"Turret": n}`, `Barbette` → `"Barbette"`,
`Bay(BaySize)` → `{"Bay": "Small"|"Medium"|"Large"}` (`ship.rs:322-340`).

Response side: `Ship` gains an optional `weapons` array in every entity broadcast.
Frontend `Ship` interface (`fe/callisto/src/lib/entities.ts:51-76`) gains
`weapons?: Weapon[]`.

New frontend helper, replacing scattered `design.weapons` reads:

```ts
export const shipWeapons = (ship: Ship, templates: ShipDesignTemplates): Weapon[] =>
  ship.weapons ?? templates[ship.design]?.weapons ?? [];
```

Frontend call sites to convert — the three helpers in
`fe/callisto/src/lib/shipDesignTemplates.ts` should take `weapons: Weapon[]` instead of a
`ShipDesignTemplate`, which localizes the churn:

- `compressedWeaponsFromTemplate` (`shipDesignTemplates.ts:41-60`)
- `findNthWeapon` (`shipDesignTemplates.ts:66-76`)
- `getWeaponName` (`shipDesignTemplates.ts:78-80`)

Their consumers: `WeaponUse.tsx:350`, `:358-368`, `:411`, `:496`, `:695-699`, `:763-770`
(these last two index `args.design.weapons[action.weapon_id]` directly);
`CrewBuilder.tsx:46`; `AddShip.tsx:254`.

---

## 6. Server-side validation (new, and needed)

Because the client now supplies weapon data, `PlayerManager::add_ship` must reject
malformed input rather than trust it. Minimum:

- `WeaponMount::Turret(n)` with `n` outside `1..=3` → return `Err`. Without this, the
  first weapon-critical hit on such a ship hits `panic!` at `ship.rs:1126` inside combat
  resolution (`combat.rs:510`), taking down the processor task.
- Cap the list length at something generous (say 64) so a hostile client cannot allocate
  unbounded weapon vectors.

I deliberately do **not** propose validating hardpoint allowance server-side — the
allowance is a UI affordance, and 11 shipped designs already violate it (§2.2).

---

## 7. Risks and open questions

**Needs the user's decision:**

1. **Small-craft floor.** I recommend "≥ 10 tons gets at least 1 hardpoint" (§2.4).
   Alternative: strict `floor(disp/100)`, which leaves 14 small craft with zero editable
   hardpoints and only the never-shrink clause keeping the 6 armed ones alive. Which?
2. **Adding beyond allowance.** I recommend rows may be *inherited* past the allowance but
   never *added*. Alternative: let the referee add freely and just warn. Which?
3. **Bays in the hardpoint dropdown** vs. a separate "Bays" section. I recommend the single
   dropdown as asked, since §2.2 shows it costs nothing in back-compat, but it does
   conflate two distinct Traveller rules.
4. **`source` filter.** I recommend deferring it (§3.1). Confirm.

**Risks:**

5. **Design tooltip diverges from reality.** `ShipDesignDetails` (`AddShip.tsx:242-303`)
   describes the *design's* armament. After editing hardpoints, the tooltip and the rows
   disagree. Recommendation: leave the tooltip design-scoped (it is a picker aid) and treat
   the hardpoint rows as the truth; consider a "modified from design" badge.
6. **Editing an in-flight ship's armament.** The "Update" path reuses `AddShip` for
   existing ships. If that ship has queued `ShipAction`s referencing `weapon_id`s, changing
   the weapon list re-points or invalidates them. `ShipActionList` lives in `Entities`
   (`entity.rs`); `add_ship`'s existing-ship branch does not currently touch it. Needs a
   decision: clear that ship's queued fire actions on armament change (safe, recommended),
   or leave them (silently wrong).
7. **No rules validation anywhere.** Tonnage, power and cost of the chosen weapons are
   never checked — a 6-ton fighter can be given a Large Particle Bay. Acceptable for a
   referee tool, but it should be a conscious choice, and `FAQ.md:21` ("Weapons larger than
   large bays are not yet supported") is the right place to record it.
8. **`Ship` `PartialEq`.** The `design` field is `#[derivative(PartialEq = "ignore")]`
   (`ship.rs:153`). The new `weapons` field is real per-ship state and should be
   *compared*, not ignored. Verify this doesn't break equality assertions in
   `src/unit_tests.rs` / `tests/webserver.rs`.
9. **Role data quality.** 54 of 79 designs are unclassified (§2.1). The "Unclassified"
   bucket keeps them reachable, but the filter is far less useful than it looks until the
   backfill in Phase 5 happens.

---

## 8. Phased plan

**Phase 1 — frontend-only picker fix. No protocol change. Ship this first.**
Add `role?` / `source?` to the TS `ShipDesignTemplate`
(`shipDesignTemplates.ts:3-19`); fix the `WeaponMount` bay type (`weapon.ts:1`); add the
role `<select>`, the text filter and `<optgroup>` grouping to `ShipDesignList`
(`AddShip.tsx:305-378`). This alone resolves "79 designs and it is unusable" and is
independently releasable.

**Phase 2 — backend per-ship armament, dormant.**
Add `Ship.weapons: Option<Vec<Weapon>>` + `Ship::weapons()`; route the 9 call sites in
§4.2; add `AddShipMsg.weapons`; add the §6 validation. No frontend change — every ship
still resolves `None` → design armament. Tests: an add-ship round-trip with an explicit
weapon list, a scenario save/load round-trip with and without the field, and a `Reset`
that preserves per-ship armament. `cargo clippy --all-targets --all-features -- -D warnings`
must be clean.

**Phase 3 — hardpoint editor UI.**
`hardpoints(design)` helper + the rows section in `AddShip.tsx`; populate from design and
re-populate on design change; compact-on-submit; send `weapons` from
`serverManager.addShip` (`serverManager.ts:312-324`). Ships built through the dialog now
carry explicit armament.

**Phase 4 — frontend read path.**
Refactor `compressedWeaponsFromTemplate` / `findNthWeapon` / `getWeaponName` to take
`Weapon[]`; add `shipWeapons()`; convert `WeaponUse.tsx` and `CrewBuilder.tsx:46`.
Until this lands, a ship with non-default armament will display its *design's* weapons in
the fire-control panel — so **Phases 3 and 4 must ship together.**

**Phase 5 — data.**
Backfill `role` (and `source` where known) on the 54 unclassified designs. Optionally add
an explicit `hardpoints: Option<u32>` to `ShipDesignTemplate` so the 11 over-allowance
designs can declare their intent instead of relying on the never-shrink clause.
