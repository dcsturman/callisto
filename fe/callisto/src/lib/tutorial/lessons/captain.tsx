import * as React from "react";
import { RoundButton } from "components/tutorial/RoundButton";
import { findShipIn, Lesson, roundPlayed } from "lib/tutorial/lesson";

export const captain: Lesson = {
  id: "captain",
  title: "The captain",
  summary: "Leadership: rolling to inspire the crew, and choosing whose work to boost.",
  scenario: "tutorial-7-captain.json",
  steps: [
    {
      id: "intro",
      title: "Command",
      body: (
        <p>
          The captain does not fly, shoot or fix anything. Each round they roll <b>Leadership</b>, and a good roll
          lets them inspire some of the crew: a +1 on that crew member&apos;s roll this round.
        </p>
      ),
    },
    {
      id: "choose-ship",
      title: "Take Killer",
      target: ["ship-menu"],
      body: <p>Killer&apos;s captain has Leadership 4.</p>,
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "evade",
      title: "Something to inspire",
      target: ["pilot-evade"],
      body: (
        <p>
          A captain boosts the crew&apos;s orders, so first give the crew one. <b>Evade</b> spends thrust on
          jinking, which makes Killer harder to hit by the pilot&apos;s skill.
        </p>
      ),
      task: (
        <>
          Set <b>Evade</b> to 1 in the Pilot card.
        </>
      ),
      done: (now) => (findShipIn(now.entities, "Killer")?.dodge_thrust ?? 0) > 0,
    },
    {
      id: "roll",
      title: "Roll Leadership",
      target: ["leadership-button"],
      body: (
        <p>
          The roll is 2D plus Leadership, less 8: the result is how many orders the captain can inspire this round.
          It can be none.
        </p>
      ),
      task: (
        <>
          Press <b>Leadership</b> in the Captain card.
        </>
      ),
      done: (now) => findShipIn(now.entities, "Killer")?.leadership_rolled === true,
    },
    {
      id: "boost",
      title: "Inspire the pilot",
      target: ["leadership-result", "boost-checkbox"],
      body: (
        <p>
          Every order that rolls has an <b>Assist</b> box in the Actions list. Tick up to as many as the captain
          inspired; the first ones ticked count.
        </p>
      ),
      task: (
        <>
          Tick the <b>Assist</b> box on the Evade line. (If the roll inspired nothing, there is nothing to tick:
          Skip, and try again another round.)
        </>
      ),
      done: (now) =>
        (now.actions["Killer"]?.leadershipCheck?.boosts ?? []).some((boost) => boost.kind === "Evade") ||
        (findShipIn(now.entities, "Killer")?.leadership_rolled === true &&
          (findShipIn(now.entities, "Killer")?.leadership_points ?? 0) <= 0),
    },
    {
      id: "round",
      title: "Play it out",
      target: ["next-round"],
      body: <p>The results include a Leadership line: who the captain inspired.</p>,
      task: (
        <>
          Press <RoundButton />.
        </>
      ),
      done: roundPlayed,
    },
    {
      id: "orders",
      title: "Standing orders",
      target: ["ship-orders"],
      body: (
        <p>
          The <b>Orders</b> box on the ship&apos;s record is the captain&apos;s standing orders, in plain words:
          &ldquo;hold the refuelling point&rdquo;, &ldquo;stay out of their gun range&rdquo;. People read them, and so
          does an AI crew.
        </p>
      ),
      task: "Write Killer some orders, then click away from the box.",
      done: (now, start) =>
        (findShipIn(now.entities, "Killer")?.orders ?? "") !== (findShipIn(start.entities, "Killer")?.orders ?? ""),
    },
    {
      id: "done",
      title: "In command",
      body: (
        <p>
          Roll every round, and put the boosts where they count most: the gunner&apos;s big shot, the
          engineer&apos;s repair, the pilot when the missiles come in. Next: leaving the fight altogether.
        </p>
      ),
    },
  ],
};
