import * as React from "react";
import { findShipIn, Lesson } from "lib/tutorial/lesson";

export const jumping: Lesson = {
  id: "jumping",
  title: "Jumping out",
  summary: "The 100-diameter limit, getting a ship ready to jump, and leaving the fight.",
  scenario: "tutorial-8-jump.json",
  steps: [
    {
      id: "intro",
      title: "The way out",
      body: (
        <p>
          A ship with a jump drive can leave the fight altogether: it jumps out of the system, and out of the
          scenario. In some scenarios that is the whole point, getting away. But a ship cannot jump close to a
          planet.
        </p>
      ),
    },
    {
      id: "choose-ship",
      title: "Take Killer",
      target: ["ship-menu"],
      body: <p>Killer is 18,000 km from Rubble, a small moonlet.</p>,
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "limit",
      title: "The 100-diameter limit",
      target: ["jump-limit-toggle"],
      body: (
        <p>
          A ship must be more than 100 diameters from <em>every</em> planet, moon and star to jump. Rubble is 200
          km across, so Killer needs to be 20,000 km out.
        </p>
      ),
      task: (
        <>
          Turn on <b>100 Diameter Limit</b> in the View panel, and look at the sphere around Rubble.
        </>
      ),
      done: (now) => now.ui.jumpDistance,
    },
    {
      id: "board",
      title: "The Jump board",
      target: ["jump-board"],
      body: (
        <p>
          The Engineer card&apos;s <b>Jump</b> board says whether Killer can jump, and checks everything it needs:
          fuel, distance from gravity, power, the Jump Control program, and the drive itself.
        </p>
      ),
    },
    {
      id: "clear",
      title: "Get clear",
      target: ["station-pilot"],
      body: (
        <p>
          Rubble is behind Killer, so straight ahead is away. One round at full thrust takes her past the limit.
        </p>
      ),
      task: (
        <>
          In the Pilot card, <b>Set Accel</b> to 6, 0, 0 and press <b>Set</b>, then press <b>Next Round</b>.
        </>
      ),
      done: (now) => findShipIn(now.entities, "Killer")?.can_jump === true,
    },
    {
      id: "software",
      title: "Jump Control",
      target: ["software-list"],
      body: (
        <p>
          A jump needs the <b>Jump Control</b> program running on the ship&apos;s computer, and Killer&apos;s computer
          is full: Evade and Fire Control are using all of it. Something has to give.
        </p>
      ),
      task: (
        <>
          In the Engineer card&apos;s <b>Computer</b> section, untick <b>Fire Control</b>, then tick{" "}
          <b>Jump Control</b>.
        </>
      ),
      done: (now) =>
        (findShipIn(now.entities, "Killer")?.software_running ?? []).some((program) => program.kind === "JumpControl"),
    },
    {
      id: "jump",
      title: "Jump",
      target: ["engineer-orders"],
      body: (
        <p>
          The engineer makes the jump: 2D plus their skill, needing 6. Even a misjump takes the ship out of the
          fight, though not to where she meant to go.
        </p>
      ),
      task: (
        <>
          Choose <b>Jump</b> in the engineer&apos;s menu, then press <b>Next Round</b>.
        </>
      ),
      done: (now, start) =>
        findShipIn(start.entities, "Killer") != null && findShipIn(now.entities, "Killer") == null,
    },
    {
      id: "done",
      title: "Gone",
      body: (
        <p>
          Killer has left the system. In a scenario where one side&apos;s goal is to <b>escape</b>, that is a win
          for them; otherwise a ship that jumps out has left the fight. Next: playing a real scenario.
        </p>
      ),
    },
  ],
};
