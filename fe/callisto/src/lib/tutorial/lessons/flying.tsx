import * as React from "react";
import { kmBetween, Lesson, planOf } from "lib/tutorial/lesson";

export const flying: Lesson = {
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
