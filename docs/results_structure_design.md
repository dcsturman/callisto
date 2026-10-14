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

Two other effects become result lines, and the **client composes their text**.
This is worth setting out, because it is not a design anyone would choose
today -- it is an order-of-arrival accident.

| | Added | What it carries |
| --- | --- | --- |
| `EffectMsg::EngineerAction` | 2026-03-04, "Added engineer role" | `ship_name`, `action`, `success`, `check`, `target`, `message`, `critical_failure` |
| `EffectMsg::LeadershipAction` | 2026-04-28, "Enable captain role" | `ship_name`, `roll`, `leadership`, `points`, `boosts_applied` |
| `EffectMsg::Message` gains structure | 2026-09-13, "Make the results log readable" | `category`, `ship`, `succeeded` |

When the engineer and captain roles were built, `Message` was a bare string.
Anything with a roll, a target number and a success flag could not be carried
by it, so each got a variant of its own -- and the rule written into
`payloads.rs` at the time still says so: *"Anything that needs real structure
gets its own `EffectMsg` variant instead."* That was right in March.

Six months later `Message` grew the structure these two were invented for.
Nothing went back to fold them in, so today:

- The client builds their sentences in `serverManager.ts`
  (`formatEngineerResult`, `formatLeadershipResult`), inventing the prefixes
  `[Engineer SUCCESS]` and `[Captain]` and naming their category itself.
- `describeBoost` has the client writing game vocabulary -- "Fire on HMS
  Executor (W#0)" -- which the server could say far better, since it knows the
  mount is a particle barbette.
- The structure that justified the variants is **unused**. The client reads
  `message`, `success` and `critical_failure` and nothing else;
  `action`, `check` and `target` are carried across the wire and dropped. The
  original intent -- "so the FE can route it through the existing action
  union" -- never happened.
- These lines cannot be filtered or coloured by ship under this design, since
  their ships are inside text.

A leadership line also names a second ship in practice: a Detection boost
reads "Death's Actuary searching for HMS Executor", so the captain's ship is
`%s` and the quarry is `%t`.

**The fix is to delete both variants** and have the server emit ordinary
`Message`s with `ship`, `target`, `roll` and `succeeded` filled in. The check
and the target number become the `roll` string that was always meant for them,
`critical_failure` picks the sentence, and the client stops writing English
anywhere. Three functions go with them: `formatEngineerResult`,
`formatLeadershipResult` and `describeBoost`.

#### What earns an effect variant of its own

The question this raises is worth writing down, because "it changes state, so
it needs structure" is the natural guess and it is not how this system works.

Effects carry **no state**. The server applies a repair to its own `Entities`
and then sends the whole snapshot; the client's ships come from that snapshot,
not from the effect. An effect is what the client should *do* about something
that happened -- and for most of them the answer is "put a line in results".

So a variant earns its place when the client must act on it in some way text
cannot express. The ones that do are the visual effects: `BeamHit` carries an
origin and an end because something has to draw the beam, `ShipImpact` and
`ShipDestroyed` carry a position because something has to go off there.
`EngineerAction` and `LeadershipAction` carry structure that nothing acts on;
their whole use is a sentence.

That cuts both ways, and the door stays open. If the damage diagram should one
day flash the section an engineer has just repaired, that is a client action
text cannot express, and the right answer is a structured effect **alongside**
the message -- the line in results, and the thing the view does -- rather than
the message carrying fields the results path has no use for. `action`,
`check` and `target` are exactly the fields such an effect would want, which
is why this design moves them out of results rather than deleting the idea.

The one thing to confirm first is that nothing else consumes the structured
fields. At the time of writing nothing does -- `serverManager.ts` is the only
reader, `EngineerActionResult` exists in `lib/entities.ts` purely to type it,
and the FE's own `EngineerAction`-named exports are a different thing (queued
orders in `actionsSlice`).

### 6.3 Tooltips: one way, not four

The client already has `react-tooltip` as a dependency and uses it in three
places -- the design picker's info icon, the ship-design details panel, and
the weapon glyphs -- with three different looks:

| Class | Looks like |
| --- | --- |
| `.tooltip-body` | green `#3dfc32` text, `--font-xs`, `padding: 2px !important` |
| `#design-tooltip` | white on grey `rgb(55,55,55)`, `--font-xs` |
| `.weapon-button-tooltip` | `.tooltip-body` plus a left pad |
| `.ship-design-description-tooltip` | a 250px cap on the rendered body |

Alongside them, 64 plain `title=` attributes across the console.

The results tooltip should not be a fifth look. Proposal:

1. **One class, `console-tip`**, defined once: the cards' own dark ground and
   hairline border, `--font-xs`, monospace where it carries numbers, a
   `max-width` around 280px, 300ms delay, no `!important`.
2. **The three existing tooltips move onto it**, keeping the design-details
   one's richer rendered body. The green-on-black and white-on-grey variants
   both go.
3. **The rule for which to use**, written down so it stops being ad hoc:
   `title` for a short plain hint; `console-tip` where the content is
   structured -- a breakdown, a column of modifiers, anything multi-line.
4. The roll arithmetic uses `console-tip`, which is what forced the question.

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
