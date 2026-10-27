import * as React from "react";
import { RoundButton } from "components/tutorial/RoundButton";
import { withdrawn, hurt, Lesson, roundPlayed } from "lib/tutorial/lesson";

export const damage: Lesson = {
  id: "damage",
  title: "Taking damage",
  summary: "Armour, hull and critical hits; the engineer's repairs, power, and overloading the drives.",
  scenario: "tutorial-6-damage.json",
  steps: [
    {
      id: "intro",
      title: "Getting hurt",
      body: (
        <>
          <p>
            Every hit loses its first few points of damage to the target&apos;s <b>armour</b>; the rest comes off
            the <b>hull</b>, and a ship with no hull left is destroyed.
          </p>
          <p>
            A big hit also does a <b>critical</b>: damage to one system, the drives, the power plant, a weapon, the
            sensors. Criticals are what make a hurt ship slower, weaker and blinder, and they are what the engineer
            fixes.
          </p>
        </>
      ),
    },
    {
      id: "raven",
      title: "Turn the guns on yourself",
      target: ["ship-menu"],
      body: (
        <p>
          Killer has no damage to fix yet, so you are going to give her some. As the GM you can work any ship: take
          <em> Raven</em>, the enemy Harrier 1,000 km off.
        </p>
      ),
      task: (
        <>
          Choose <em>Raven</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Raven",
    },
    {
      id: "shoot",
      title: "Fire on Killer",
      target: ["fire-target", "weapon-buttons"],
      body: <p>A particle barbette at Short range: Killer is going to feel this.</p>,
      task: (
        <>
          Choose <em>Killer</em> as the target, click the <b>Particle Barbette</b>, and press <RoundButton />. If
          Raven misses, press it again.
        </>
      ),
      done: (now, start) => hurt(now.entities, "Killer") > hurt(start.entities, "Killer"),
    },
    {
      id: "cease",
      title: "That will do",
      target: ["captain-actions"],
      body: <p>Raven would keep firing every round. Call her off before she finishes the job.</p>,
      task: (
        <>
          Close the results, then click the barbette&apos;s line under <b>Actions</b>.
        </>
      ),
      done: (now, start) => withdrawn(now, start, "Raven", (f) => f.target === "Killer"),
    },
    {
      id: "killer",
      title: "Back to Killer",
      target: ["ship-menu"],
      body: <p>Now see what Raven did.</p>,
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "damage-board",
      title: "Damage control",
      target: ["damage-board"],
      body: (
        <>
          <p>
            The Engineer card&apos;s <b>Damage control</b> shows the whole ship. The drawing colours each system:
            green is sound, amber is hurt, red is badly hurt. Below it is a line for each damaged system, its
            severity in pips, and the DM a repair would roll at.
          </p>
          <p>
            Hull and armour are at the bottom. They cannot be repaired in a fight; every other system can.
          </p>
        </>
      ),
    },
    {
      id: "repair",
      title: "Repair",
      target: ["engineer-orders"],
      body: (
        <p>
          An engineer does one thing a round, chosen here. A repair rolls 2D plus their skill, minus the damage,
          and needs 8. Each failure makes the next try easier; each success takes one level off.
        </p>
      ),
      task: (
        <>
          Choose a <b>Repair</b> in the engineer&apos;s menu. (If it says <b>No damaged systems to repair</b>, the
          hit only took hull: Skip this step.)
        </>
      ),
      done: (now) => (now.actions["Killer"]?.engineers ?? []).some((order) => order?.kind === "Repair"),
    },
    {
      id: "repair-round",
      title: "Make the repair",
      target: ["next-round"],
      body: <p>The results will say whether it worked.</p>,
      task: (
        <>
          Press <RoundButton />.
        </>
      ),
      done: roundPlayed,
    },
    {
      id: "power",
      title: "Power",
      target: ["power-board"],
      body: (
        <p>
          The <b>Power</b> board is a budget: the plant&apos;s output against what the drives, sensors and weapons
          draw. A hurt power plant gives less, and then something has to be switched off. The ON/OFF buttons do
          that, at the cost of the engineer&apos;s order for the round.
        </p>
      ),
    },
    {
      id: "overload",
      title: "Overload",
      target: ["engineer-orders"],
      body: (
        <p>
          When you need more than the ship has, the engineer can <b>Overload Drive</b> for an extra G, or{" "}
          <b>Overload Plant</b> for more power, for a few rounds. It needs 10+, each try after the first is harder,
          and a bad failure damages the system instead.
        </p>
      ),
    },
    {
      id: "done",
      title: "Patched up",
      body: (
        <p>
          That is the engineer: read the damage, repair what matters most, balance the power, and push the drives
          when you must. Next: the captain.
        </p>
      ),
    },
  ],
};
