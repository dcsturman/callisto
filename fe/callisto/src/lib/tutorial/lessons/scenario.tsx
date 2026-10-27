import * as React from "react";
import { ViewMode } from "lib/view";
import { fromPost } from "lib/posts";
import { rememberSides, timeUp } from "lib/victory";
import { findShipIn, Lesson, TutorialContext } from "lib/tutorial/lesson";

const atPost = (now: TutorialContext) => {
  const killer = findShipIn(now.entities, "Killer");
  return killer != null && fromPost(now.entities, killer)?.inside === true;
};

export const scenario: Lesson = {
  id: "scenario",
  title: "Playing a scenario",
  summary: "Starting and joining games, taking a seat, Ready, goals, posts, and how a game is won.",
  scenario: "tutorial-9-scenario.json",
  steps: [
    {
      id: "games",
      title: "Games",
      body: (
        <>
          <p>
            Outside the tutorial, every game starts on the scenario screen. <b>Create New Scenario</b> starts one
            from a scenario file, and gives it a three-word name, like <em>ocean-river-lamp</em>. Tell your friends
            that name: they find it under <b>Join Existing Scenario</b>.
          </p>
          <p>Everyone in a game sees the same battle, each from their own seat.</p>
        </>
      ),
    },
    {
      id: "session",
      title: "Your seat",
      target: ["session-panel"],
      body: (
        <p>
          The session panel in the bottom corner shows the game&apos;s name, who else is playing, and your{" "}
          <b>seat</b>. On a smaller screen it starts folded to one line: click the game&apos;s name to open it.
          The GM seat sees and works everything, and ends the rounds. Everyone else takes a ship.
        </p>
      ),
    },
    {
      id: "take-seat",
      title: "Take a ship",
      target: ["seat-menu"],
      body: <p>Taking a ship gives you its console, and its crew&apos;s view of the battle: only what they have found.</p>,
      task: (
        <>
          Choose <em>Killer</em> in the seat menu (open the session panel first if it is folded).
        </>
      ),
      done: (now) => now.user.shipName === "Killer",
    },
    {
      id: "role",
      title: "Take a station",
      target: ["role-menu"],
      body: (
        <p>
          With five friends on one ship, each takes a station: pilot, sensors, gunner, engineer, captain. The role
          menu narrows your console to yours. <b>General</b> is every station at once.
        </p>
      ),
      task: (
        <>
          Choose <b>Pilot</b> in the role menu, and see the console shrink to the Pilot card.
        </>
      ),
      done: (now) => now.user.roles.includes(ViewMode.Pilot),
    },
    {
      id: "general",
      title: "All hands",
      target: ["role-menu"],
      body: <p>For the rest of this lesson you will want every station.</p>,
      task: (
        <>
          Choose <b>General</b> in the role menu.
        </>
      ),
      done: (now, start) => !start.user.roles.includes(ViewMode.General) && now.user.roles.includes(ViewMode.General),
    },
    {
      id: "ready",
      title: "Ready",
      target: ["next-round"],
      body: (
        <p>
          A player with a ship has <b>Ready</b> instead of Next Round: press it when your orders are in. With a GM,
          the GM runs the round; without one, it runs when every player is Ready. Here you are the only player.
        </p>
      ),
    },
    {
      id: "goal",
      title: "Your goal",
      target: ["post-readout"],
      body: (
        <>
          <p>
            Not every side is out to destroy the other. Killer&apos;s side is to <b>hold</b> a post: the purple ring
            on the screen, around the Outpost moonlet. The Ships panel says how far she is from it.
          </p>
          <p>
            The other goal is <b>escape</b>: get the side&apos;s ships out of the system by jumping (lesson 8)
            before they are destroyed. It is an escape if more than two-thirds of them get away, unless the
            scenario sets its own number; if time runs out with enough of them still alive but not yet out, it is
            a draw. A scenario&apos;s description says what each side is for.
          </p>
        </>
      ),
    },
    {
      id: "station",
      title: "Take your post",
      target: ["nav-target", "post-readout"],
      body: (
        <p>
          Plot a course to the Outpost with a <b>Standoff</b> of 2,000 km (you cannot park on a moon), assign it,
          and fly. The Ships panel says &ldquo;at post&rdquo; when you are there.
        </p>
      ),
      task: "Get Killer inside the purple ring, pressing Ready to play each round.",
      done: atPost,
    },
    {
      id: "hold",
      title: "Hold to the end",
      target: ["round-counter"],
      body: (
        <p>
          This scenario lasts eight rounds: the counter says <b>Round N of 8</b>. A side holding a post wins if a
          ship of theirs is still at it when time runs out. The attackers have to come and take it.
        </p>
      ),
      task: "Stay at your post, and press Ready until the rounds run out.",
      done: (now, start) => timeUp(now.entities, rememberSides(new Map(), start.entities.ships)) != null,
    },
    {
      id: "done",
      title: "Held!",
      body: (
        <>
          <p>
            Time is up and Killer is at her post, so Blue has held, and the victory screen says so. It offers{" "}
            <b>Keep Going</b>, to play on past the end, or <b>Exit Scenario</b>, which here takes you back to the
            lessons.
          </p>
          <p>
            Games also end when only one side has ships left. Next: playing with your own fleet, and with AI
            crews.
          </p>
        </>
      ),
    },
  ],
};
