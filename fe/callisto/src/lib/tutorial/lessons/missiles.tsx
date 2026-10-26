import * as React from "react";
import { fireOrders, withdrawn, Lesson, roundPlayed } from "lib/tutorial/lesson";

/** The System Defense Boat's weapons, by mount: 0 the beam turret, 1 the missile turret. */
const BEAMS = 0;
const MISSILES = 1;

export const missiles: Lesson = {
  id: "missiles",
  title: "Missiles",
  summary: "Launching missiles, watching them fly, and shooting down the ones coming at you.",
  scenario: "tutorial-5-missiles.json",
  steps: [
    {
      id: "intro",
      title: "Missiles",
      body: (
        <>
          <p>
            A missile is a small ship of its own. It accelerates at 10 G, steers itself towards its target every
            round, and burns for up to ten rounds. Range does not matter to a missile, only whether it can get
            there.
          </p>
          <p>
            You command <em>Warden</em>, a system defence boat with a beam laser turret and a missile turret. The
            courier <em>Raider</em> is 18,000 km off, and she has already fired.
          </p>
        </>
      ),
    },
    {
      id: "choose-ship",
      title: "Take Warden",
      target: ["ship-menu"],
      body: <p>The red specks on the screen heading for Warden are Raider&apos;s missiles.</p>,
      task: (
        <>
          Choose <em>Warden</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Warden",
    },
    {
      id: "incoming",
      title: "Incoming",
      target: ["incoming"],
      body: (
        <p>
          The Gunner card&apos;s <b>Salvoes</b> board lists every missile coming at you: whose they are, and when
          they arrive. These hit this round.
        </p>
      ),
    },
    {
      id: "point-defence",
      title: "Point defence",
      target: ["fire-target", "weapon-buttons"],
      body: (
        <>
          <p>
            A laser in a turret can shoot down missiles instead of shooting at ships. Each round it rolls once
            (8+ with the gunner&apos;s skill), and a success destroys at least one missile: more, the better the
            roll.
          </p>
          <p>The target menu has a special entry for it, <b>&lt;Point Defense&gt;</b>.</p>
        </>
      ),
      task: (
        <>
          Choose <b>&lt;Point Defense&gt;</b> as the target, then click the <b>beam laser turret</b>.
        </>
      ),
      done: (now) => (now.actions["Warden"]?.pointDefense ?? []).some((pd) => pd.weapon_id === BEAMS),
    },
    {
      id: "launch",
      title: "Fire back",
      target: ["fire-target", "weapon-buttons"],
      body: (
        <p>
          Launching is just like firing a gun: choose the ship, then the launcher. There is no roll to launch;
          the missile rolls to hit when it arrives.
        </p>
      ),
      task: (
        <>
          Choose <em>Raider</em> as the target, then click the <b>missile turret</b>.
        </>
      ),
      done: (now) => fireOrders(now, "Warden").some((f) => f.weapon_id === MISSILES && f.target === "Raider"),
    },
    {
      id: "magazine",
      title: "The magazine",
      target: ["magazine"],
      body: (
        <p>
          Missiles are counted. The magazine shows how many Warden has left, and every launch uses some. A
          launcher keeps launching every round until you call it off, so keep an eye on it.
        </p>
      ),
    },
    {
      id: "launch-round",
      title: "Launch",
      target: ["next-round"],
      body: (
        <p>
          This round your point defence fires at Raider&apos;s missiles as they come in, and your missiles leave
          their tubes. At 18,000 km they take a round to get there.
        </p>
      ),
      task: (
        <>
          Press <b>Next Round</b>, then look for Warden&apos;s missiles on the screen.
        </>
      ),
      done: (now) => now.entities.missiles.some((m) => m.source === "Warden"),
    },
    {
      id: "results",
      title: "How the defence did",
      target: ["results"],
      body: (
        <p>
          The results say how many missiles your point defence could stop, how many it destroyed, and what the
          rest did. Missiles roll to hit when they arrive: 8+, with a bonus for being more advanced than their
          target, +2 with a sensor lock, and less for the target&apos;s evasion. Press <b>Okay!</b> to close them.
        </p>
      ),
    },
    {
      id: "cease",
      title: "Save your missiles",
      target: ["captain-actions"],
      body: <p>One salvo in flight is enough for now.</p>,
      task: (
        <>
          Click the missile turret&apos;s line under <b>Actions</b> to stop launching.
        </>
      ),
      done: (now, start) => withdrawn(now, start, "Warden", (f) => f.weapon_id === MISSILES),
    },
    {
      id: "impact",
      title: "Impact",
      target: ["next-round"],
      body: <p>Watch your missiles close on Raider and strike.</p>,
      task: (
        <>
          Press <b>Next Round</b>.
        </>
      ),
      done: roundPlayed,
    },
    {
      id: "done",
      title: "Missiles, done",
      body: (
        <>
          <p>
            Launch, watch them fly, and keep a laser on point defence when the other side launches. Two more
            defences: the sensor operator&apos;s <b>Jam Missiles</b> order, and <b>sand</b>, which some ships
            throw automatically (it stops lasers, not missiles).
          </p>
          <p>Next: what happens when the shooting goes the other way.</p>
        </>
      ),
    },
  ],
};
