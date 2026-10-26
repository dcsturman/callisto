import * as React from "react";
import { findShipIn, Lesson, TutorialContext } from "lib/tutorial/lesson";

const names = (c: TutorialContext) => [...c.entities.ships, ...c.entities.planets].map((e) => e.name);

export const builder: Lesson = {
  id: "builder",
  title: "Building a scenario",
  summary: "The Scenario Builder: planets, ships, sides and crews, and saving.",
  scenario: "tutorial-11-builder.json",
  builder: true,
  steps: [
    {
      id: "welcome",
      title: "The Scenario Builder",
      target: ["builder-controls"],
      body: (
        <>
          <p>
            Every scenario you have played was made here. The builder is not a game: nothing moves and there are no
            rounds. You place planets and ships, give them sides and crews, and save the result for people to play.
          </p>
          <p>
            You have a planet, Haven, to start from. In the tutorial, <b>Save</b> checks your scenario but does not
            keep it, so experiment freely.
          </p>
        </>
      ),
    },
    {
      id: "planet",
      title: "Add a planet",
      target: ["builder-add-planet"],
      body: (
        <p>
          Planets need a name, a position in km, a radius in km, and a mass. A <b>Primary</b> makes it orbit another
          body: a moon around Haven, say.
        </p>
      ),
      task: (
        <>
          Open <b>Add Planet</b>, give it a name, a position and a radius, and press <b>Add</b>.
        </>
      ),
      done: (now, start) => now.entities.planets.length > start.entities.planets.length,
    },
    {
      id: "ship",
      title: "Add a ship",
      target: ["builder-add-ship", "builder-ship-design"],
      body: (
        <p>
          A ship needs a name, a position and velocity, and a <b>Design</b>: one of the ship designs, from fighters
          to cruisers. Hover over a design to see its details.
        </p>
      ),
      task: (
        <>
          Open <b>Add Ship</b>, choose a Design, and press <b>Add</b>.
        </>
      ),
      done: (now, start) => now.entities.ships.length > start.entities.ships.length,
    },
    {
      id: "team",
      title: "Pick a side",
      target: ["builder-ship-team"],
      body: (
        <p>
          Every ship in a battle is on a side: <b>Red</b>, <b>Blue</b>, <b>Green</b> or <b>Gold</b>. To change a
          ship you have added, type her name into <b>Name</b>: the form loads her, and Add becomes <b>Update</b>.
        </p>
      ),
      task: (
        <>
          Put your ship on a side with <b>Team</b>, and press <b>Update</b>.
        </>
      ),
      done: (now) => now.entities.ships.some((ship) => ship.team != null),
    },
    {
      id: "opponent",
      title: "An opponent",
      target: ["builder-add-ship", "builder-ship-team"],
      body: <p>A battle needs two sides.</p>,
      task: "Add a second ship, on a different side.",
      done: (now) => new Set(now.entities.ships.map((ship) => ship.team).filter((team) => team != null)).size >= 2,
    },
    {
      id: "crew",
      title: "The crew",
      target: ["builder-crew"],
      body: (
        <p>
          Each ship&apos;s crew starts with the skills its design suggests. Set them for your story: a green crew
          is 0 or 1, veterans 3 or more. Sensor operators and engineers can be added or removed; gunners are set
          with each weapon.
        </p>
      ),
      task: (
        <>
          Change one of a ship&apos;s crew skills, and press <b>Update</b>.
        </>
      ),
      done: (now, start) =>
        now.entities.ships.some((ship) => {
          const before = findShipIn(start.entities, ship.name);
          return before != null && JSON.stringify(before.crew) !== JSON.stringify(ship.crew);
        }),
    },
    {
      id: "ai",
      title: "AI crews and orders",
      target: ["builder-add-ship"],
      body: (
        <p>
          On a server with AI crews, Add Ship also has <b>AI crew</b>: tick it and the ship is crewed by the AI
          whenever the scenario is played, with the <b>Orders</b> you write for her. Players can still change who
          crews each side when they start a game.
        </p>
      ),
    },
    {
      id: "contents",
      title: "What's in it",
      target: ["builder-entity-list"],
      body: (
        <p>
          <b>Scenario contents</b> lists everything you have placed. The pencil renames a ship or planet; the bin
          removes it.
        </p>
      ),
      task: "Rename something with its pencil, and press Enter.",
      done: (now, start) =>
        names(now).length === names(start).length && names(start).some((name) => !names(now).includes(name)),
    },
    {
      id: "goals",
      title: "Goals, posts and time limits",
      body: (
        <p>
          A ship&apos;s goal (destroy, hold, raid or escape), the post a holding side defends, and a round limit
          are not in the builder yet. They are set in the scenario file itself: the scenarios that come with
          Callisto show how.
        </p>
      ),
    },
    {
      id: "save",
      title: "Save",
      target: ["builder-save"],
      body: (
        <p>
          Saving asks for a file name, a <b>Display Name</b> (what players see in the scenario list) and a{" "}
          <b>Description</b> (what they read before playing: say who is who, and what each side is for).
        </p>
      ),
      task: (
        <>
          Press <b>Save Scenario</b>, fill it in, and press <b>Save</b>.
        </>
      ),
      done: (now, start) => now.ui.scenarioSaves > start.ui.scenarioSaves,
    },
    {
      id: "done",
      title: "For real",
      body: (
        <>
          <p>
            Outside the tutorial, the scenario screen&apos;s <b>Scenario Builder</b> button starts a blank scenario,
            or opens an existing one to change. A save is kept, and appears in everyone&apos;s scenario list within
            a few seconds.
          </p>
          <p>
            Saving over an existing file asks first, and only the person who made a scenario can save over it.
            That&apos;s the end of the tutorial.
          </p>
        </>
      ),
    },
  ],
};
