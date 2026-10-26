import * as React from "react";
import { fireOrders, withdrawn, kmBetween, Lesson, roundPlayed } from "lib/tutorial/lesson";

/** The Harrier's weapons, by mount: 0 the particle barbette, 1 the missile turret. */
const BARBETTE = 0;

export const guns: Lesson = {
  id: "guns",
  title: "Guns",
  summary: "Ranges, firing, the odds, reading the results, and called shots.",
  scenario: "tutorial-4-guns.json",
  steps: [
    {
      id: "intro",
      title: "Gunnery",
      body: (
        <>
          <p>
            Killer carries a <b>particle barbette</b>, a heavy beam weapon, and a <b>missile turret</b>. This lesson
            is the barbette; missiles have the next lesson to themselves.
          </p>
          <p>
            Beowulf is 15,000 km away, and Killer already has her as a contact, so you can open fire straight away.
          </p>
        </>
      ),
    },
    {
      id: "choose-ship",
      title: "Take Killer",
      target: ["ship-menu"],
      body: <p>Every gun on a ship is worked from the Gunner card.</p>,
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "ranges",
      title: "Range",
      target: ["target-board"],
      body: (
        <>
          <p>Range is in bands, and every band changes the odds:</p>
          <dl className="coach-keys">
            <dt>Short</dt>
            <dd>to 1,250 km, DM+1</dd>
            <dt>Medium</dt>
            <dd>to 10,000 km</dd>
            <dt>Long</dt>
            <dd>to 25,000 km, DM−2</dd>
            <dt>Very Long</dt>
            <dd>to 50,000 km, DM−4</dd>
            <dt>Distant</dt>
            <dd>beyond, DM−6</dd>
          </dl>
          <p>
            Each weapon has a longest band it can reach. A particle barbette reaches Very Long; a beam laser only
            Medium. The Targets board lists the enemy you have found, with the band each is in.
          </p>
        </>
      ),
    },
    {
      id: "fire",
      title: "Open fire",
      target: ["fire-target", "weapon-buttons"],
      body: (
        <p>
          A gun fires at whatever is chosen in <b>target:</b>. The weapon buttons stay grey until there is a target.
        </p>
      ),
      task: (
        <>
          Choose <em>Beowulf</em> as the target, then click the <b>Particle Barbette</b> button.
        </>
      ),
      done: (now) => fireOrders(now, "Killer").some((f) => f.target === "Beowulf" && f.weapon_id === BARBETTE),
    },
    {
      id: "odds",
      title: "The odds",
      target: ["shot-odds"],
      body: (
        <>
          <p>
            <b>Show shot odds</b> works out your chance before you commit. A hit needs 8 or more on 2D, plus the
            gunner&apos;s skill, the weapon, the range, +2 with a sensor lock, and minus the target&apos;s evasion.
          </p>
          <p>
            Damage is the weapon&apos;s dice plus how much the shot beat 8 by (the <b>Effect</b>), less the
            target&apos;s armour.
          </p>
        </>
      ),
    },
    {
      id: "queued",
      title: "Your orders",
      target: ["captain-actions"],
      body: (
        <p>
          Every order for the round is listed under <b>Actions</b> in the Captain card. A gun keeps firing at its
          target every round until you call it off, by clicking its line here.
        </p>
      ),
    },
    {
      id: "round",
      title: "Fire!",
      target: ["next-round"],
      body: <p>Shots are fired at the start of the round, from where the ships are now, before anyone moves.</p>,
      task: (
        <>
          Press <b>Next Round</b>.
        </>
      ),
      done: roundPlayed,
    },
    {
      id: "results",
      title: "What happened",
      target: ["results"],
      body: (
        <p>
          The Results window lists every roll of the round: hit or miss, the Effect, the damage. Beowulf&apos;s
          hull shows how much she has left in the Ships panel. Press <b>Okay!</b> to close it.
        </p>
      ),
    },
    {
      id: "cease",
      title: "Cease fire",
      target: ["captain-actions"],
      body: <p>Time to close in, and there is no point wasting shots at long range while you do.</p>,
      task: (
        <>
          Click the barbette&apos;s line under <b>Actions</b> to call it off.
        </>
      ),
      done: (now, start) => withdrawn(now, start, "Killer", (f) => f.weapon_id === BARBETTE),
    },
    {
      id: "close",
      title: "Close to Short range",
      target: ["nav-target"],
      body: (
        <p>
          At Short range you get DM+1 and something better: called shots. Use the Pilot card&apos;s navigation to
          plot a course to Beowulf with a <b>Standoff</b> of 1,000 km, so you stop just inside Short.
        </p>
      ),
      task: "Fly to within 1,250 km of Beowulf: plot, assign, and press Next Round until you are there.",
      done: (now) => kmBetween(now.entities, "Killer", "Beowulf") < 1250,
    },
    {
      id: "called",
      title: "A called shot",
      target: ["called-shot", "captain-actions"],
      body: (
        <p>
          At Short range a direct-fire weapon can aim for one system, at DM−2. If the hit causes a critical, the
          first one lands where you aimed: the <b>Powerplant</b> to cut her power, the <b>Maneuver</b> drive to
          stop her running.
        </p>
      ),
      task: (
        <>
          Fire the barbette at Beowulf again, then choose a system in its <b>No called shot</b> menu under Actions.
        </>
      ),
      done: (now) =>
        fireOrders(now, "Killer").some((f) => f.weapon_id === BARBETTE && f.called_shot_system != null),
    },
    {
      id: "take-it",
      title: "Take the shot",
      target: ["next-round"],
      body: <p>Then check the results for where the hit went.</p>,
      task: (
        <>
          Press <b>Next Round</b>.
        </>
      ),
      done: roundPlayed,
    },
    {
      id: "done",
      title: "Guns, done",
      body: (
        <p>
          Choose a target, fire, read the results, and get close for the shots that matter. Lasers work the same
          way, but they can be blunted by <b>sand</b>, which some ships throw automatically: you will see it listed
          under <b>Automatic</b>. Next: missiles.
        </p>
      ),
    },
  ],
};
