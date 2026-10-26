import * as React from "react";
import { RoundButton } from "components/tutorial/RoundButton";
import { Lesson } from "lib/tutorial/lesson";

/** How far the camera has to fly before "look around" counts, in scene units. */
const CAMERA_MOVE = 0.5;
/** How far the camera has to turn, as a change in its quaternion (about 10°). */
const CAMERA_TURN = 0.09;

export const gettingAround: Lesson = {
  id: "getting-around",
  title: "Getting around",
  summary: "The screen, the panels, choosing a ship, and playing a round.",
  scenario: "tutorial-1-getting-around.json",
  steps: [
    {
      id: "welcome",
      title: "Welcome to Callisto",
      body: (
        <>
          <p>
            Callisto is space combat in the Traveller rules: ships, planets and missiles moving under real
            physics, a few minutes of game time at a round.
          </p>
          <p>
            This panel is your coach. Each step explains one thing. When a step asks you to do something, it
            waits, ticks when you have done it, and moves on. You can go back a step, or skip one.
          </p>
        </>
      ),
    },
    {
      id: "look-around",
      title: "Look around",
      body: (
        <>
          <p>The main screen is space itself. You fly the camera, not a ship:</p>
          <dl className="coach-keys">
            <dt><kbd>W</kbd> <kbd>S</kbd></dt><dd>forward and back</dd>
            <dt><kbd>A</kbd> <kbd>D</kbd></dt><dd>left and right</dd>
            <dt><kbd>R</kbd> <kbd>F</kbd></dt><dd>up and down</dd>
            <dt><kbd>Q</kbd> <kbd>E</kbd></dt><dd>roll</dd>
            <dt>arrows</dt><dd>pitch and yaw</dd>
            <dt><kbd>Shift</kbd></dt><dd>faster</dd>
            <dt>drag</dt><dd>look around</dd>
          </dl>
          <p>The keys are always in the View panel, under Camera Keys.</p>
        </>
      ),
      task: "Move the camera: fly it somewhere, or turn it to look another way.",
      done: (now, start) =>
        Math.hypot(...now.ui.cameraPos.map((p, i) => p - start.ui.cameraPos[i])) > CAMERA_MOVE ||
        Math.hypot(...now.ui.cameraQuaternion.map((q, i) => q - start.ui.cameraQuaternion[i])) > CAMERA_TURN,
    },
    {
      id: "ships-panel",
      title: "The ships",
      target: ["ships-panel"],
      body: (
        <>
          <p>
            The Ships panel lists every ship you can see, coloured by side: here your <em>Killer</em> and a
            trader, <em>Beowulf</em>. Each line gives its hull, its thrust, and how far it is from you.
          </p>
          <p>
            In a big scenario this list gets long. Click its heading to fold it away; click again to bring it
            back. Every panel on the right folds the same way.
          </p>
        </>
      ),
    },
    {
      id: "choose-ship",
      title: "Take a ship",
      target: ["ship-menu"],
      body: (
        <p>
          You are playing as the GM here, so you can work any ship. Choosing one opens its console: everything
          its crew can do.
        </p>
      ),
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "dossier",
      title: "Killer's record",
      target: ["ship-dossier"],
      body: (
        <>
          <p>
            The top of the console is the ship&apos;s record: her design (a Harrier), hull, armour, sensors,
            where she is and how fast she is going.
          </p>
          <p>
            Callisto&apos;s units: positions in <b>kilometres</b>, speeds in <b>metres per second</b>, and
            acceleration in <b>G</b> (1 G is 9.8 m/s²).
          </p>
        </>
      ),
    },
    {
      id: "stations",
      title: "The crew stations",
      target: ["station-captain", "station-pilot", "station-sensors", "station-gunner", "station-engineer"],
      body: (
        <>
          <p>
            Below the record are the crew&apos;s stations, one card each: <b>Captain</b>, <b>Pilot</b>,{" "}
            <b>Sensors</b>, <b>Gunner</b>, <b>Engineer</b>, and the <b>Targets</b> and <b>Incoming</b> boards.
          </p>
          <p>
            In a game with friends, each player usually takes one station on one ship. As the GM you see them
            all. Fold a card by its heading, and move it up or down with its arrows to put what you use most on
            top. The next lessons go through them one at a time.
          </p>
        </>
      ),
    },
    {
      id: "view",
      title: "The view",
      target: ["view-panel"],
      body: (
        <p>
          The View panel changes what the main screen draws. <b>Gravity Well</b> shows each planet&apos;s pull.{" "}
          <b>100 Diameter Limit</b> shows how close to a planet is too close to jump.
        </p>
      ),
      task: "Turn on Gravity Well.",
      done: (now) => now.ui.gravityWells,
    },
    {
      id: "rounds",
      title: "Rounds",
      target: ["next-round", "round-counter"],
      body: (
        <>
          <p>
            Callisto is played in rounds of six minutes. Everyone gives their orders; then the round runs, and
            every ship, missile and planet moves at once.
          </p>
          <p>
            The GM ends each round with <b>Next Round</b>. In a game without a GM, every crew presses{" "}
            <b>Ready</b> instead, and the round runs when they all have. The counter shows which round it is,
            and in some scenarios how many there are.
          </p>
        </>
      ),
      task: (
        <>
          Press <RoundButton /> twice, and watch the Moon move.
        </>
      ),
      done: (now, start) => (now.entities.round ?? 1) >= (start.entities.round ?? 1) + 2,
    },
    {
      id: "done",
      title: "That's the screen",
      body: (
        <p>
          You know your way around: the view, the panels, a ship&apos;s console and the round. Next, make{" "}
          <em>Killer</em> go somewhere.
        </p>
      ),
    },
  ],
};
