# Mixed Turrets

Status: **design proposal, nothing implemented.**
Date: 2026-09-08.
Rules source: *MgT2 Core Rulebook* 2022 update, "Double and Triple Turrets", p. 166.

---

## 1. The rule

> Some spacecraft are fitted with double or triple turrets, which allow two or three
> weapons to be mounted in the same turret. **If these weapons are different** (a pulse
> laser, missile rack and sandcaster in the same triple turret, for example), then **only
> one type may be used in a single combat round.**
>
> However, if two or more weapons are of the same type, they may be fired together. One
> attack roll is made for all weapons being fired, but **each additional weapon adds +1 per
> damage dice** to the final damage total.
>
> […] Sandcasters can also be linked in this way, granting +1 to the damage negated by
> laser attacks for each additional sandcaster beyond the first.
>
> Missiles are handled differently when in double or triple turrets, so **do not get the
> bonus** above.

### 1.1 What we already implement correctly

The same-type bonus is **already right**, which narrows this job considerably:

- `combat.rs` adds `(num - 1) * damage_dice` for a `Turret(num)`, which is precisely "each
  additional weapon adds +1 per damage dice". The book's worked example — a triple pulse
  turret dealing 2D+4 — comes out exactly.
- Missiles are excluded, because the turret bonus sits in the non-launcher branch.
- Sandcasters get `n - 1` in `create_sand_counts`, matching the linked-sandcaster clause.

So none of the firing arithmetic changes. What is missing is only the ability to *represent*
a turret holding different weapons, and the one-type-per-round restriction that comes with it.

### 1.2 The divergence this leaves today

Mixed turrets in the library were split into uniform ones during conversion. That is not
neutral — **it hands the ship more capability than the book allows.**

The MK Mora is the clear case. The book gives it six triple turrets, each holding two pulse
lasers and a sandcaster. Per the rule, each turret fires *either* its lasers *or* its
sandcaster in a given round; the ship must choose. Our split gives it four pulse triple
turrets and two sandcaster triple turrets, so it fires **12 pulse lasers and 6 sandcasters
every round, simultaneously**. There is no round in the book where it can do that.

This is recorded as a deliberate conversion decision, but it is a live rules divergence
rather than a cosmetic one, and it is the main argument for doing this work at all.

---

## 2. The schema question

Today one `Weapon` **is** a whole mount: `Turret(3)` with `kind: Beam` means one turret
holding three beam lasers, and it occupies one entry in `Ship::weapons()`. `weapon_id` is an
index into that list, so it already means "which mount", not "which gun". That is the right
foundation and none of the options below disturb it.

### Option A — every mount holds a list of guns

```rust
pub struct Gun {
  pub kind: WeaponType,
  pub modifiers: Vec<WeaponModifier>,
}

pub struct Weapon {
  pub mount: WeaponMount,
  pub guns: Vec<Gun>,
}
```

A triple beam turret is three identical `Gun`s. A mixed turret is two pulse and one sand.
Barbettes, bays and fixed mounts hold exactly one.

### Option B — an enum with a uniform and a mixed variant

```rust
pub enum Weapon {
  Uniform { kind: WeaponType, mount: WeaponMount, modifiers: Vec<WeaponModifier> },
  Mixed   { mount: WeaponMount, guns: Vec<Gun> },
}
```

Today's shape is preserved untouched for the common case, and mixed turrets are opt-in.

### 2.1 Recommendation — Option A

**B's fatal problem is that it has two representations for the same ship.** A `Mixed` turret
holding three identical beam lasers *is* a uniform triple beam turret, but the two values are
not equal, do not hash alike and do not group together. That matters concretely:

- `Ord for Weapon` and `PartialEq` are used to sort and compare armament.
- The Add Ship editor groups weapons by `(mount, kind, gunnery, modifiers)` into counted
  rows. Two spellings of the same turret would produce two rows.
- `compressedWeapons` on the frontend does the same thing for the design summary.

Every one of those needs a normalization step under B, and each is a place where forgetting
it produces a quiet wrong answer rather than an error. Option A cannot express the
distinction at all, so there is nothing to normalize.

B's advantage — leaving the common path untouched — is real but temporary. A is a large
one-time churn; B is a permanent second code path that every future weapon feature has to
handle twice.

**Churn, measured rather than guessed:** 42 `.kind` accesses across the Rust source (28
outside tests) and 66 across the frontend. Most are mechanical (`weapon.kind` becomes a
lookup over `guns`), but they are not all trivial — see §3.

### 2.2 `Turret(n)` becomes redundant

Under A, a turret's gun count is `guns.len()`, so `WeaponMount::Turret(u8)` carries a second
source of truth for the same fact. They can disagree.

**Recommend dropping the payload:** `WeaponMount::Turret`. Nothing consults the number except
the same-type bonus and the sand and point-defence counts, all of which should count guns of
the relevant type anyway — which is the mixed-turret rule. `MountClass` already collapses
`Turret(n)`, and hardpoint accounting charges 1 per turret regardless of size.

This does mean validating `guns.len()` per mount: 1–3 for a turret, exactly 1 for a fixed
mount, barbette or bay.

### 2.3 Wire compatibility

Option A changes the shape of every weapon in every design and scenario file. That is
avoidable: read both forms with an untagged helper and convert on the way in.

```rust
#[derive(Deserialize)]
#[serde(untagged)]
enum WeaponWire {
  /// Today's shape, still written by every existing design file.
  Uniform { kind: WeaponType, mount: WeaponMount, #[serde(default)] modifiers: Vec<WeaponModifier> },
  /// The new shape, used only where a mount holds different weapons.
  Guns { mount: WeaponMount, guns: Vec<Gun> },
}
```

A `Turret(3)` in the old form expands to three identical `Gun`s on load. Serialization
should write the **old** form whenever every gun matches, so the 79 design files stay
byte-identical and only genuinely mixed turrets gain the new spelling. That keeps the diff
of this change to code plus the one design that needs it.

---

## 3. The part that is not schema

Storage is the easy half. The rule *"only one type may be used in a single combat round"*
means a mixed turret must be told **which type it is firing**, and nothing in the system
currently carries that.

- **`FireAction { weapon_id, target, called_shot_system }`** needs a gun selector — a
  `WeaponType`, or an index into `guns`. It is optional: a uniform turret has no choice to
  make. That is a wire change to actions and to `merge`, which already treats fire and
  point-defence actions as mutually exclusive per `weapon_id`.
- **The same-type bonus** becomes `(count of the firing type - 1) * damage_dice` rather than
  `(turret size - 1)`.
- **`point_defense_score`** must count *laser* guns in the turret, not all guns. A turret
  with one pulse laser and two sandcasters is a weak point-defence mount, not a strong one.
- **`create_sand_counts`** likewise counts sandcaster guns.
- **Profiles** are looked up per `(kind, mount)`; a mixed turret has several kinds, so the
  lookup moves to per-gun. Modifiers are already per-gun, which fits.
- **Frontend**: the fire button for a mixed turret has to offer a type choice, and the mount
  icon has to represent a turret with two kinds in it. This is the one place where a new
  visual is genuinely needed rather than nice to have.

**This, not the struct change, is where the risk is.** The schema can be landed and proved
with no behaviour change at all; the firing restriction cannot.

---

## 3.5 The gunner's action budget

This is the half of the problem the firing restriction alone does not cover, and it is the
reason mixed turrets are worth having at all.

### 3.5.1 What the rules say

- **One action per crew member per round.** "Once all ships have resolved their attacks,
  their crew can perform one action each in the Actions Step" (CRB p. 170).
- **Ship combat does not have a general reaction economy.** Its REACTIONS section (p. 170)
  only says reactions "can only be performed by Travellers assigned to specific duties" and
  then defines three, each carrying its own limit. The unlimited-reactions-with-cumulative-
  DM-1 rule is from *personal* combat (p. 75) and does **not** apply here; an earlier draft
  of this document wrongly imported it.
- **Evasive Action (pilot):** one attempt per point of unspent Thrust.
- **Point Defence (gunner):** "only once every round", and "a weapon used for point defence
  **cannot be used to make attacks in the same combat round and vice versa**" (p. 171). That
  exclusion is on the *weapon*.
- **Disperse Sand (gunner):** adds 1D + Effect to armour "against that laser attack only",
  and "each Disperse Sand reaction uses one canister of sand" (p. 171). No per-round cap is
  stated, unlike point defence.

Callisto already resolves the sand question in practice: `create_sand_counts` builds one
entry per sandcaster and each incoming laser attack consumes one, so a sandcaster defends
**once per round**. That is the reading this design keeps.

**Adopted: one attack and one reaction per mount per round.**

### 3.5.2 Why mixed turrets force this to be modelled

Callisto currently models **one gunner per weapon** without ever saying so: `Crew::gunnery`
is an array indexed by weapon, and every weapon may fire every round. With only uniform
mounts that is indistinguishable from a per-gunner budget, so the question never came up.

A mixed turret breaks that. One turret is one gunner, and the guns inside it share that
gunner's action and reaction. So the budget has to move from **per weapon** to **per mount**:

- one **attack**, using one gun type (the firing restriction of §1);
- one **reaction** — Disperse Sand or Point Defence — which may use a *different* gun.

That is a real change to the action model, not bookkeeping, and it is the first time
Callisto needs the concept of a gunner as distinct from a weapon.

### 3.5.3 Why this makes sandcasters the obvious third gun

A triple turret of two pulse lasers and a sandcaster lets its gunner attack with the lasers
**and** still hold a defensive reaction, from one Hardpoint. That is a strictly better use of
the third barrel than a gun the gunner has no action left to fire, and it is why most mixed
turrets in practice will pair an attacking weapon with sand.

The same logic applies to a laser paired with a *missile rack*: the gunner attacks with one
and the other sits idle, which is a worse buy. This is worth stating because it predicts
which mixed turrets referees will actually build, and therefore which combinations the
editor and the fire UI need to make easy.

### 3.5.4 What a mount may do in a round — decided

The Double and Triple Turrets rule says "only one type may be **used** in a single combat
round". Read literally that would forbid firing the lasers *and* dispersing sand from the
same turret, which makes the sandcaster in §3.5.3 useless and the mount pointless to build.

**Decided (2026-09-08):**

| | |
| --- | --- |
| **Attack** | One per mount, using one gun type. |
| **Reaction** | One per mount. |
| **Disperse Sand as the reaction** | **Allowed even when the mount attacked with a different gun.** This is the whole point of putting a sandcaster in a mixed turret. |
| **Point Defence as the reaction** | **Only if the mount did not attack this round.** |

So the restriction is read as governing *attacks*, with one deliberate carve-out and one
deliberate tightening.

**Why sand is allowed alongside an attack.** Without it a sandcaster in a mixed turret can
never be used except by giving up the turret's attack, which no referee would ever choose.
The point-defence rule bothering to state its own weapon-level exclusion also suggests the
turret rule is about firing rather than every use — that clause would be redundant for mixed
turrets otherwise.

**Why point defence is not.** A turret holding a missile rack and a laser could otherwise
attack with the missile and run point defence with the laser in the same round, getting two
distinct combat effects out of one gunner and one Hardpoint. That is arguably legal under
the same reading that permits sand, but it is a bigger step and a stronger combination, so
it stays out for now. Sand is a pure defensive add-on; laser point defence is a second
active use of the mount.

This also makes the "how many reactions" question moot in practice: a mount gets one, and
the only configuration that would want two — sand *and* laser point defence alongside a
missile attack — is exactly the case ruled out above. If that tightening is ever relaxed,
the one-reaction cap is what would need revisiting first.

## 4. This is not the batteries work

Worth stating plainly, because I previously suggested they might be the same refactor and
that was wrong. They are different axes:

- A **mixed turret** is one mount holding heterogeneous guns.
- A **battery** (TODO 5) is *many mounts*, homogeneous, aimed and resolved as one attack.

Option A does not deliver batteries, and batteries do not deliver mixed turrets. They
compose cleanly — a battery of mixed turrets is coherent — but neither is a step toward the
other.

---

## 5. Suggested phasing

1. **Schema only.** Option A with the compatibility reader, `Turret` losing its payload,
   `guns.len()` validation, and serialization that still writes the old form for uniform
   mounts. Every existing design loads and plays **exactly** as it does today; no combat
   behaviour changes. This is the large, boring, provable step.
2. **Counting by type.** Same-type damage bonus, sand counts and point-defence scores count
   guns of the relevant type. Still no behaviour change while every turret is uniform.
3. **The firing restriction and the action budget.** The `FireAction` gun selector, the
   one-type-per-round rule, one attack and one reaction per mount, and the frontend choice.
   This is where play changes -- and note the budget applies to uniform mounts too, so it is
   the first step that can alter an existing ship's behaviour.
4. **Re-convert the MK Mora** to its book armament — six mixed turrets rather than the split
   — and drop the deliberate ±1 rounding recorded in `modified designs.md`.

Steps 1 and 2 are safe by construction: with no mixed turret in the library, they cannot
change any outcome. Step 3 is the one that wants care and a scenario to test against.

---

## 6. Open questions

- **Should a mount be allowed to attack with one gun and run point defence with another?**
  Ruled out for now in §3.5.4, but it is the natural next question if mixed turrets prove
  popular.
- **Is step 4 wanted?** Re-converting the MK Mora makes it *weaker* — it loses the ability
  to fire lasers and sand in the same round. That is faithful, but it changes a ship someone
  may already be flying.
- **Should the editor let a referee build mixed turrets**, or only display ones that come
  from a design? Building them needs a per-gun editor inside a turret row, which is a real
  piece of UI.
- **Is `Turret` losing its payload acceptable**, given it appears in every design file?
  Serialization can keep writing `Turret(n)` for uniform mounts, so the answer is probably
  yes, but it is worth confirming before the churn.
