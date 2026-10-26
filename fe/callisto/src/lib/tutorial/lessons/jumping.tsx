import * as React from "react";
import { RoundButton } from "components/tutorial/RoundButton";
import { findShipIn, Lesson } from "lib/tutorial/lesson";

export const jumping: Lesson = {
  id: "jumping",
  title: "Computer control & jumping out",
  summary: "The ship's computer and its programs; the 100-diameter limit, and jumping out of the fight.",
  scenario: "tutorial-8-jump.json",
  steps: [
    {
      id: "intro",
      title: "The way out",
      body: (
        <>
          <p>
            Two things in this lesson. First the ship&apos;s computer: the programs it runs shape how a ship
            fights, and one of them is needed to jump.
          </p>
          <p>
            Then the jump itself. A ship with a jump drive can leave the fight altogether: it jumps out of the
            system, and out of the scenario. In some scenarios that is the whole point, getting away. But a ship
            cannot jump close to a planet.
          </p>
        </>
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
        <>
          <p>
            The Engineer card&apos;s <b>Jump</b> board is a checklist. At the top is the verdict:{" "}
            <b>Ready to jump</b>, <b>Cannot jump</b>, or <b>Ready — jump-N only</b> when the ship can go, but not
            as far as her drive is rated for. Below it, a line for everything a jump needs, each with a ✓ when it
            is met, a ✕ when it stops the jump, or a ⚠ when it only shortens it:
          </p>
          <ul>
            <li>
              <b>Fuel</b>: a jump burns a tenth of the ship&apos;s tonnage per jump number. Short of that, she can
              still jump, but less far.
            </li>
            <li>
              <b>Clear of gravity</b>: how far she is from the nearest planet, against its 100-diameter limit.
            </li>
            <li>
              <b>Power</b>: the drive draws power at the moment of the jump, so there must be enough spare.
            </li>
            <li>
              <b>Jump Control</b>: the computer program that plots the jump. Not running, no jump.
            </li>
            <li>
              <b>Astrogation</b>: the course, plotted by the bridge and computer. Bridge or computer damage stops
              it.
            </li>
            <li>
              <b>Drive</b>: the jump drive itself, and how far it can take her now. Damage shortens it.
            </li>
          </ul>
          <p>
            Killer has two ✕s: she is only 18,000 km from Rubble, inside its 20,000 km limit, and Jump Control is
            not running. The next two steps fix them.
          </p>
        </>
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
          In the Pilot card, <b>Set Accel</b> to 6, 0, 0 and press <b>Set</b>, then press <RoundButton />.
        </>
      ),
      done: (now) => findShipIn(now.entities, "Killer")?.can_jump === true,
    },
    {
      id: "computer",
      title: "The computer",
      target: ["computer-board"],
      body: (
        <>
          <p>
            The Engineer card&apos;s <b>Computer</b> section is the ship&apos;s computer. Its rating,{" "}
            <b>Computer/20</b> on a Harrier, is its <em>Bandwidth</em>: how much software it can run at once.
            Every program costs Bandwidth, more at higher levels, and the bar shows how much is in use.
          </p>
          <p>
            Tick a program to run it and untick it to stop; it takes effect straight away. The ones that matter in
            a fight:
          </p>
          <ul>
            <li>
              <b>Evade</b>: the computer jinks the ship, a penalty to every attack on her.
            </li>
            <li>
              <b>Fire Control</b>: a bonus to the gunners, or guns that fire themselves.
            </li>
            <li>
              <b>Jump Control</b>: needed to jump at all.
            </li>
            <li>
              <b>Electronic Warfare</b>: a bonus to the sensor operator&apos;s jamming and lock-breaking.
            </li>
            <li>
              <b>Battle Network</b>: lets a ship hand off its contacts without spare Bandwidth.
            </li>
            <li>
              <b>Auto-Repair</b>: repairs by drone, on ships that carry repair drones.
            </li>
          </ul>
          <p>
            Bandwidth left spare is not wasted: sensor hand-offs need some free on both ships. Bridge damage, or
            an ion hit, takes Bandwidth away, and then something has to be shut down.
          </p>
        </>
      ),
    },
    {
      id: "software",
      title: "Jump Control",
      target: ["software-list"],
      body: (
        <p>
          Killer&apos;s computer is full: Evade and Fire Control are using all 20 of its Bandwidth, and Jump Control
          needs 10. Something has to give, and with no enemy here, it is the guns.
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
      id: "ready",
      title: "Ready to jump",
      target: ["jump-board"],
      body: (
        <p>
          Every line on the Jump board should now be ticked, and the verdict read <b>Ready to jump</b>. If one still
          shows ✕, it says why. One catch: the server only marks a ship clear of gravity at the end of a round, so
          the engineer&apos;s <b>Jump</b> order is offered the round after she gets clear, even when the board
          already shows the ✓.
        </p>
      ),
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
          Choose <b>Jump</b> in the engineer&apos;s menu, then press <RoundButton />.
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
