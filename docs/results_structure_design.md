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
Message {
  /// The sentence, with `%s` where the subject ship's name goes and `%t`
  /// where the target's does. Still server-authored English; the client
  /// never builds a sentence.
  content: String,
  category: MessageCategory,
  /// The subject: the ship acting, or the ship the fact is about.
  ship: Option<String>,
  /// The object, where the message has one: the ship shot at, jammed,
  /// detected, handed a contact.
  target: Option<String>,
  /// How the number came out, lifted out of `content`: "roll 7 + skill 4 +
  /// assist 2 = 13 vs 8". Shown on hover rather than in the line.
  roll: Option<String>,
  succeeded: Option<bool>,
}
```

`ship` and `target` carry **names**, which is what the rest of the protocol
uses as a ship's identity.

### 2.1 Why `%s` and `%t` rather than `{ship}`

Every one of these sentences is built with `format!`, where `{ship}` would have
to be written `{{ship}}` and read badly at eighty call sites. `%s`/`%t` pass
through `format!` untouched. They are also not valid Rust format syntax, so a
placeholder left unsubstituted cannot be mistaken for one.

### 2.2 Compatibility

Both new fields are `Option` with `#[serde(default)]`, and a `content` with no
placeholder in it renders exactly as it does today. That makes the conversion
incremental: the type changes once, then call sites move over a category at a
time, and nothing is broken in between.

---

## 3. How a line is drawn

```
✓  HMS Executor particle beam → Cicada: hit, effect 0. Damage 4D 10 = 30.
   ^^^^^^^^^^^^                ^^^^^^
   bold, team colour           bold, team colour
```

- **Both ship names bold**, always.
- **Coloured by team**: the four team colours, or grey when the ship is on no
  team. This is the rule the ship list and contact detail already use
  (`teamLabelColor`), so results agree with the rest of the console.
- **The rest of the line** keeps its category tint, as now.
- **A line with roll arithmetic** gets a quiet marker — a dotted underline
  under the sentence — and the arithmetic on hover.

### 3.1 Where the colour comes from

A ship destroyed this round is gone from the entity list by the time its
results are read, and those are exactly the lines that matter most. Two
choices:

1. **Resolve from entities, fall back to grey.** No new wire data; the ship
   that just died loses its colour.
2. **Send the team with the message** — `ship_team`, `target_team` — two small
   fields the server already has at the moment it writes the line.

This design takes (2). It costs two enum fields and makes a result
self-contained, which is the property that makes results worth keeping after
the round that produced them.

---

## 4. Roll arithmetic on hover

Today the sentence carries it:

> HMS Executor leadership check with roll 7 and skill +4 for a total of 11
> against 8: 3 points; 1 boost(s): Fire HMS Executor#0.

Under this design the line reads:

> **HMS Executor** inspires 3 tasks: fire control on her own particle barbette.

and the hover carries *"roll 7 + leadership 4 = 11 vs 8"*.

The attack lines are the bigger win, since their breakdown is already itemised
and already long:

> **HMS Executor** particle beam → **Cicada**: hit, effect 0. Damage 4D 10 = 30.

hover: *"3 + 5 = 8 vs 8 — gunner +3, assist +2, captain fire +1, range +1,
called shot −2"*.

`roll` is a **string**, not a structure. The server already composes these
breakdowns for the current sentences, so a string is a move rather than a
rewrite; and the client does nothing with the parts but print them. If a later
need appears — sorting by DM, say — a structured form can be added beside it.

---

## 5. Filtering

A compact row of chips at the top of the results window, one per ship named in
this round's results, each in its team colour:

```
 ships:  [HMS Executor] [Cicada] [Death's Actuary] [Invigilator]      all
```

- **Multi-select**, not a dropdown: a referee watching a boarding action cares
  about two ships, not one.
- **Default: all on**, so the panel behaves as it does today until someone
  touches it.
- **A line matches if either end matches** — picking Executor shows what she
  did *and* what was done to her, which is what "messages about this ship"
  means to a player.
- Chips appear only for ships in the current results, so the row stays short.
- Selection lives in `uiSlice` and persists across rounds, like the card order.
  A ship that leaves the scenario drops out of the row; if that empties the
  selection, it resets to all rather than showing nothing.

Open: whether an unselected line should be **hidden** or **dimmed**. Hidden is
what was asked for and is what a filter normally means. Dimmed keeps the shape
of the round — you can still see that something happened elsewhere — at the
cost of the compression that makes filtering worth having.

---

## 6. Open questions

### 6.1 The three-ship message

*"{recipient} receives contact on {contact} from {host}"* has three ships.
Options:

- **a.** `%s`/`%t` plus a third named field for the contact (`%c`). Clear, but
  the third field exists for one message.
- **b.** An ordered list of referents, `%1 %2 %3`, with `ship`/`target` kept as
  the first two for filtering. General, slightly more machinery.
- **c.** Leave the third name plain text. It then neither bolds nor filters —
  and "which ships is this about" is exactly the question the hand-off line
  raises.

### 6.2 Is `ship` required or optional?

Once the seven subject-less sites are given their obvious subject, is any
message genuinely about no ship? *"X is out of range of Y's Z"* and the range
band lines both have a natural subject. If none remain, `ship` could become
required, which makes the filter total and removes a branch from the client. A
required field is also a forcing function on whoever writes the next message.

### 6.3 Hover mechanism

`title` is what the rest of the console uses: free, accessible, no layout work.
It is also slow to appear, cannot be styled, and does not exist on a touch
screen. A styled tooltip would read better on the one panel where the
arithmetic matters. Consistency argues for `title`; this is the panel that
might deserve the exception.

### 6.4 Emphasis inside the sentence

The question raised with this work: are bold ship names enough to make a round
readable, or does the line need its *point* emphasised too — the damage number,
"miss", the critical's severity?

Three ways:

- **a.** Nothing further. See how the ship names read first.
- **b.** One emphasis span per message, marked in `content` (`*30*` or
  `%e{30}`), used for the thing that changed. Cheap, and the server already
  knows which number that is.
- **c.** Structured outcome fields (damage, effect, severity) and let the
  client lay them out. Most work, most consistency, and it starts to make the
  sentence a template.

Recommendation: ship 1–3, read a real round, then decide between (a) and (b).
The whole point of this work is that the line should be scannable; whether it
is cannot be judged from here.

### 6.5 Category filtering

Should the same row offer "combat only" or "hide detection"? The categories
exist and a round of sensor checks is the noisiest thing in the log. Probably a
second row of chips, probably later — but worth knowing now, because it decides
whether the filter state is "a set of ships" or "a set of predicates".

---

## 7. Work, in order

1. **Wire type and constructors** — add `target`, `roll`, the two team fields;
   extend `about`/`outcome` with variants that take a target and a roll.
   Nothing renders differently yet.
2. **Client** — substitute `%s`/`%t`, bold and colour them, hang `roll` on a
   hover. Lines without placeholders keep working, so this can land before any
   message is converted.
3. **Convert the call sites**, a category at a time: Attack and Damage first
   (the longest lines and the most arithmetic), then Detection and Handoff,
   then Engineering, Leadership, Critical, Destruction, Info.
4. **Filtering** in `uiSlice` and the results window.
5. **The seven subject-less messages** get their subject, and `ship` becomes
   required if 6.2 lands that way.

Tests: the integration tests assert on whole message strings. They will need a
helper that renders a message the way the client does — substituting the names
— so the assertions stay readable and the placeholders are checked by the same
act.
