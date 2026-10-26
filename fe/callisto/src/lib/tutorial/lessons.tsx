import * as React from "react";
import { kmBetween, Lesson, planOf } from "lib/tutorial/lesson";

/** How far the camera has to fly before "look around" counts, in scene units. */
const CAMERA_MOVE = 0.5;
/** How far the camera has to turn, as a change in its quaternion (about 10°). */
const CAMERA_TURN = 0.09;

const gettingAround: Lesson = {
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
      task: "Press Next Round twice, and watch the Moon move.",
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

const flying: Lesson = {
  id: "flying",
  title: "Flying",
  summary: "Plotting a course with the navigation computer, and flying it.",
  scenario: "tutorial-2-flying.json",
  steps: [
    {
      id: "newton",
      title: "Ships keep going",
      body: (
        <>
          <p>
            Ships in Callisto obey Newton. A ship keeps its speed until it burns to change it, so getting
            somewhere means speeding up, then turning around and slowing down so as to arrive, not fly past.
          </p>
          <p>
            Working that out by hand is hard. The pilot&apos;s navigation computer does it for you.
          </p>
        </>
      ),
    },
    {
      id: "choose-ship",
      title: "Take Killer",
      target: ["ship-menu"],
      body: <p>Killer is sitting still near Earth. Beowulf is 25,000 km away and drifting.</p>,
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "pilot",
      title: "The pilot",
      target: ["station-pilot"],
      body: (
        <>
          <p>The Pilot card flies the ship. From the top:</p>
          <ul>
            <li>
              <b>Evade</b> and <b>Assist Gunner</b> spend thrust on dodging fire or steadying the guns, which
              leaves less for moving.
            </li>
            <li><b>Set Accel</b> burns in a direction you type, in G.</li>
            <li><b>Full Stop</b> plots a course that brings you to a halt.</li>
            <li><b>Navigation</b> plots a course to a ship or planet.</li>
          </ul>
        </>
      ),
    },
    {
      id: "compute",
      title: "Plot a course",
      target: ["nav-target"],
      body: (
        <p>
          Choosing a Nav Target fills in where it is and how it is moving. The computer then plots a course that
          meets it and matches its speed, within your ship&apos;s thrust.
        </p>
      ),
      task: (
        <>
          In Navigation, choose <em>Beowulf</em> as the Nav Target and press <b>Compute</b>.
        </>
      ),
      done: (now) => now.ui.proposedPlan != null,
    },
    {
      id: "assign",
      title: "Read the plan",
      target: ["proposed-plan"],
      body: (
        <>
          <p>
            The proposed plan is drawn on the main screen as an orange line: where Killer will go, curving to
            meet Beowulf where she will be, not where she is now.
          </p>
          <p>
            A plan is two burns: one to speed up towards the target, and one to slow down. Each burn is a
            direction and a time. If the drives take damage, the plan can no longer push as hard.
          </p>
        </>
      ),
      task: (
        <>
          Press <b>Assign Plan</b> to give it to Killer.
        </>
      ),
      done: (now, start) => planOf(now.entities, "Killer") !== planOf(start.entities, "Killer"),
    },
    {
      id: "fly",
      title: "Fly it",
      target: ["next-round"],
      body: (
        <p>
          Killer now flies her plan, round by round. It takes about four rounds to close the distance. Watch the
          range to Beowulf drop in the Ships panel.
        </p>
      ),
      task: "Press Next Round until Killer is within 2,000 km of Beowulf.",
      done: (now) => kmBetween(now.entities, "Killer", "Beowulf") < 2000,
    },
    {
      id: "standoff",
      title: "Stopping short",
      target: ["nav-target"],
      body: (
        <>
          <p>
            Arriving on top of a ship is not always what you want: a fight might be better at a distance. The{" "}
            <b>Standoff</b> box sets how far short of the target the course ends, in km.
          </p>
          <p>
            And to run away, plot a course to the enemy, then turn the burn around: put the same numbers into
            Set Accel with their signs flipped.
          </p>
        </>
      ),
    },
    {
      id: "stop",
      title: "Full stop",
      target: ["full-stop"],
      body: <p>Killer matched Beowulf&apos;s speed, so she is still drifting with her. To stop dead:</p>,
      task: (
        <>
          Press <b>Full Stop</b>, then <b>Assign Plan</b>.
        </>
      ),
      done: (now, start) => planOf(now.entities, "Killer") !== planOf(start.entities, "Killer"),
    },
    {
      id: "done",
      title: "You can fly",
      body: (
        <p>
          That is all of navigation: choose a target, compute, assign, and run the rounds. Next comes finding
          the enemy with your sensors.
        </p>
      ),
    },
  ],
};

/** Every lesson, in the order they are taught. */
export const LESSONS: Lesson[] = [gettingAround, flying];

export const lessonById = (id: string | null): Lesson | undefined => LESSONS.find((lesson) => lesson.id === id);

/** The lesson after this one, if there is one. */
export const nextLesson = (id: string): Lesson | undefined => LESSONS[LESSONS.findIndex((l) => l.id === id) + 1];
