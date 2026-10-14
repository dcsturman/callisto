# Structured results

Status: **design only, nothing implemented.**
Date: 2026-10-14.

Scope: `callisto/src/payloads.rs` (the wire type and its constructors),
roughly eighty `EffectMsg` call sites across `callisto/src/combat.rs`,
`callisto/src/entity.rs` and `callisto/src/player.rs`, plus
`fe/callisto/src/components/space/Effects.tsx` (the results window),
`fe/callisto/src/state/uiSlice.ts` (filter state) and `fe/callisto/src/index.css`.

---

## 1. Where we are

A result is one `EffectMsg::Message`, and today it carries four things:

```rust
Message {
  content: String,            // the whole sentence, server-authored English
  category: MessageCategory,  // Attack, Damage, Critical, Detection, Handoff,
                              // Engineering, Leadership, Destruction, Info
  ship: Option<String>,       // the subject, where there is one
  succeeded: Option<bool>,    // tick or cross
}
```

The client prints `content` verbatim, tinted by `category`, with a tick or a
cross in the margin.

Two of the four pieces this design needs are therefore already in place:
`category` and `succeeded`. What is missing is any way to find the ships a line
is about without reading the sentence, and any way to get the arithmetic out of
the sentence.

**The call sites.** 74 of the 81 construct a message with a subject ship
(`EffectMsg::about`, 66; `EffectMsg::outcome`, 8). Seven use
`EffectMsg::message`, which has no subject — and of those, most have an obvious
one that simply was not passed, for example *"X's weapon is not a mount this
weapon can be fired from"* and *"X's point defence batteries will intercept up
to N missile(s) this round"*. So the answer to "is there a ship in every
message?" is: nearly, and the handful that are missing one are oversights
rather than a different kind of message.

**Two ships is common.** Attacks (`attacker weapon -> target`), ion drain,
jamming, detection checks and range bands all name a second ship inside the
sentence.

**Three ships happens once.** A hand-off reads *"{recipient} receives contact
on {contact} from {host}"* — three distinct ships, none of them decoration.
This is the case that decides whether `%t` is enough (section 6.1).

---

## 2. The wire type

```rust
/// A ship named in a result: who it was, and whose side it was on when the
/// line was written.
///
/// The team travels with the message because a ship destroyed this round is
/// gone from the entity list by the time its results are read -- and those
/// are the lines that matter most. A result that carries its own colours is
/// a result that still reads tomorrow.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ShipRef {
  pub name: String,
  pub team: Option<Team>,
}

Message {
  /// The sentence, with placeholders where the ships go: `%s` the subject,
  /// `%t` the target, `%c` the third party. Server-authored English; the
  /// client substitutes names and never builds a sentence of its own.
  content: String,
  /// What kind of event this is. Tints the line, and the referee filters
  /// on it.
  category: MessageCategory,
  /// The ship this is about: the one acting, or the one the fact is about.
  /// Required -- every message in the codebase has one (section 6.2).
  ship: ShipRef,
  /// The ship acted upon, where there is one: shot at, jammed, detected,
  /// handed a contact.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  target: Option<ShipRef>,
  /// A third ship the sentence names. Today that is the hand-off and only
  /// the hand-off: "%s receives contact on %c from %t".
  #[serde(default, skip_serializing_if = "Option::is_none")]
  contact: Option<ShipRef>,
  /// How the number came out: "roll 7 + leadership 4 = 11 vs 8". Lifted out
  /// of `content` and shown on hover.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  roll: Option<String>,
  /// Tick, cross, or neither.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  succeeded: Option<bool>,
}
```

Constructors stay short at eighty call sites by building rather than taking
seven arguments:

```rust
EffectMsg::about(ship, MessageCategory::Attack, content)
  .at(target)        // sets `target`
  .on(contact)       // sets `contact`
  .rolled(breakdown) // sets `roll`
  .outcome(hit)      // sets `succeeded`
```

### 2.1 Why `%s` and `%t` rather than `{ship}`

Every one of these sentences is built with `format!`, where `{ship}` would have
to be written `{{ship}}` and read badly at eighty call sites. `%s`/`%t`/`%c`
pass through `format!` untouched. They are also not valid Rust format syntax,
so a placeholder left unsubstituted cannot be mistaken for one.

### 2.2 Compatibility

`target`, `contact` and `roll` are optional with serde defaults, and a
`content` with no placeholder renders as it does today. `ship` becoming
required is the one breaking change, and it is a compile error at each site
rather than a silent one.

---

## 3. How a line is drawn

```
✓  HMS Executor particle beam → Cicada: hit, effect 0. Damage 4D 10 = 30.
   ^^^^^^^^^^^^                ^^^^^^
   bold, team colour           bold, team colour
```

- **Every ship name bold**, always -- subject, target and contact alike.
- **Coloured by team**, or grey when the ship is on no team. Same rule the
  ship list and contact detail use, so results agree with the rest of the
  console.
- **The rest of the line** keeps its category tint.
- **A line with arithmetic** carries a quiet marker and gives the arithmetic
  on hover.

---

## 4. Roll arithmetic on hover

Today the sentence carries it:

> HMS Executor leadership check with roll 7 and skill +4 for a total of 11
> against 8: 3 points; 1 boost(s): Fire HMS Executor#0.

Under this design the line reads:

> **HMS Executor** inspires 3 tasks: fire control on her own particle barbette.

with *"roll 7 + leadership 4 = 11 vs 8"* on hover. The attack lines gain more,
since their breakdown is already itemised and already long.

`roll` is a **string**, not a structure. The server composes these breakdowns
today, so a string is a move rather than a rewrite, and the client only prints
it. A structured form can be added beside it if anything ever needs the parts.

### 4.1 `title` or a tooltip of our own

**`title`** is the browser's built-in: set an attribute, the browser draws a
small box after about a second. It costs nothing, screen readers announce it,
and it is what the rest of the console uses. Against it: the delay, no styling
at all (system font, system colours, the browser's own wrapping), and **it does
not exist on a touch screen** -- there is no hover to have.

**A tooltip of our own** is an element we position and style: appears at once,
in the console's monospace so a column of modifiers lines up, as wide as the
breakdown needs, and can be opened by click or tap as well as hover. Against
it: it is code, it needs care inside a panel that scrolls and clips, and it has
to handle keyboard focus to stay accessible.

Recommendation: **our own, for this one panel.** The arithmetic is a list of
named numbers -- "gunner +3, assist +2, captain fire +1, range +1, called shot
-2" -- which is exactly what the browser's tooltip renders worst. Everything
else in the console keeps `title`.

---

## 5. Filtering

Two rows of chips at the head of the results window, both multi-select, both
hiding what is not chosen:

```
 ships:       [HMS Executor] [Cicada] [Death's Actuary] [Invigilator]   all
 categories:  [Attack] [Damage] [Critical] [Detection] [Handoff] ...    all
```

- **Ships** carry their team colour, and a line matches if **any** of its
  three ships is chosen -- picking Executor shows what she did and what was
  done to her.
- **Categories** let the sensor operator watch sensors and nothing else.
- The two combine with **and**: chosen ships, chosen categories. Sensors on
  one ship is the case this is for.
- **Unchosen lines are hidden**, not dimmed.
- **Default is everything**, so the panel behaves as it does today until
  someone touches it.
- Chips appear only for ships and categories present in the current results,
  so the rows stay short.
- Selection lives in `uiSlice` beside the card order, and persists. A ship
  that leaves the scenario drops out; if that would empty the selection, it
  resets to all rather than showing an empty panel.

---

## 6. What the audit found

Every `EffectMsg` construction in the server, read:

| Category | Sites |
| --- | --- |
| Critical | 41 |
| Attack | 8 |
| Detection | 8 |
| Damage | 7 |
| Handoff | 6 |
| Destruction | 2 |
| Engineering | 1 |
| Info | 1 |
| *(no category, no subject)* | 7 |
| **Total** | **81** |

### 6.1 Every message has a subject

The seven with no subject today, and the subject each should have:

| Where | Message | Subject |
| --- | --- | --- |
| `combat.rs:155` | "%s's %w is not a mount this weapon can be fired from" | the firing ship |
| `combat.rs:249` | "%t is out of range of %s's %w" | the firing ship |
| `combat.rs:1444` | "%s's %w cannot be fired from that mount" | the firing ship |
| `combat.rs:1460` | "%t is out of range of %s's %w" | the firing ship |
| `entity.rs:880` | "%s's point defence batteries will intercept up to N" | the defending ship |
| `entity.rs:1153` | "missile destroyed by %s's point defence" | the defending ship |
| `payloads.rs:832` | a unit-test fixture, not a message the server sends | — |

So `ship` can be required: there is no message in the codebase that is about no
ship. Two of them do read with the target as the grammatical subject -- "Cicada
is out of range of Executor's beam" -- but the rule is the ship *acting*, which
keeps the filter meaning one thing.

### 6.2 Results do not all come from `Message`

Two other effects become result lines, and the **client composes their text**:
`EffectMsg::EngineerAction { result }` and `EffectMsg::LeadershipAction { .. }`
are turned into Message-shaped events in `serverManager.ts`, where the client
also invents their prefixes ("[Engineer SUCCESS]", "[Captain]") and names their
category.

That has to move. If the client keeps writing sentences, half the results have
ships inside text nobody can bold and arithmetic nobody can lift out. The
server should emit these as ordinary `Message`s with `ship`, `roll` and
`succeeded` filled in -- which also deletes `formatEngineerResult` and
`formatLeadershipResult` and the only place the client writes English.

A check before doing it: whether anything else in the client reads
`EngineerActionResult`'s structured fields. At the time of writing it does not
-- `serverManager.ts` is the only consumer, and only to make a line.

---

## 7. Still open

- **Emphasis inside the sentence.** Deferred by decision: ship names first,
  then read a real round and decide whether the line's *point* -- the damage,
  "miss", the severity -- needs bolding too. If it does, the cheap form is one
  marked span per message, since the server knows which number matters.
- **Whether the hand-off is the only three-ship line.** `contact` is in the
  type for it; if a second case appears the field is already general.

## 8. Work, in order

1. **Wire type and constructors** — `ShipRef`, the `target`/`contact`/`roll`
   fields, and the builder. `ship` becomes required, which is a compile error
   at each of the eighty-one sites rather than a silent change.
2. **Client** — substitute `%s`/`%t`/`%c`, bold and colour them from the
   `ShipRef`s, hang `roll` on a tooltip. Lines with no placeholder keep
   working, so this lands before any message is converted.
3. **Convert the call sites**, a category at a time: Attack and Damage first —
   the longest lines and the most arithmetic — then Detection and Handoff, then
   Critical, Engineering, Destruction, Info.
4. **Move the two client-composed families server-side** (section 6.2), so
   every result is a `Message` and the client writes no English at all.
5. **Filtering**: two chip rows, ships and categories, state in `uiSlice`.
6. **Emphasis**, if a real round still reads badly.

Tests: the integration tests assert on whole message strings. They need a
helper that renders a message the way the client does — substituting names —
so the assertions stay readable and the placeholders get checked by the same
act.
