import * as React from "react";
import { findShipIn, Lesson, TutorialContext } from "lib/tutorial/lesson";

/** Whether this game has AI crews: the server has an AI service, so Kite came in crewed by it. */
const aiHere = (start: TutorialContext) => start.entities.ships.some((ship) => ship.controller === "AI");

export const fleet: Lesson = {
  id: "fleet",
  title: "Your own fleet",
  summary: "AI crews, Fleet Comms, Fleet Command, and taking a ship back from the AI.",
  scenario: "tutorial-10-fleet.json",
  steps: [
    {
      id: "intro",
      title: "More ships than players",
      body: (
        <p>
          Plenty of scenarios have more ships than there are people to fly them. Two things help: <b>AI crews</b>,
          which fly a ship for you, and <b>Fleet Command</b>, which lets one player run a whole side.
        </p>
      ),
    },
    {
      id: "no-ai",
      title: "AI crews",
      when: (start) => !aiHere(start),
      body: (
        <>
          <p>
            This server has no AI service, so there are no AI crews in this lesson. On a server with one, the
            scenario screen&apos;s <b>Crews</b> section sets each side to <b>Human</b>, <b>AI</b> or <b>Mixed</b>{" "}
            (some ships each) when you create a game.
          </p>
          <p>
            An AI-crewed ship gives all its own orders every round, and explains them in the <b>Fleet Comms</b>{" "}
            panel. Anyone can take her back for people to crew, or hand her to the AI, from the <b>Crew</b> line
            on her record.
          </p>
        </>
      ),
    },
    {
      id: "crews",
      title: "AI crews",
      when: aiHere,
      body: (
        <p>
          On the scenario screen, the <b>Crews</b> section sets each side to <b>Human</b>, <b>AI</b> or{" "}
          <b>Mixed</b> when you create a game. In this one, Blue&apos;s <em>Kite</em> and Red&apos;s{" "}
          <em>Raven</em> are crewed by the AI, and give their own orders every round.
        </p>
      ),
    },
    {
      id: "comms",
      title: "Fleet Comms",
      when: aiHere,
      target: ["fleet-comms"],
      body: (
        <p>
          The AI captains report here: each side&apos;s plan, and each ship&apos;s reason for her orders. As the
          GM you hear every side; a player hears only their own.
        </p>
      ),
      task: (
        <>
          Press <b>Next Round</b>, and read what the AI captains say.
        </>
      ),
      done: (now, start) => (now.entities.round ?? 1) > (start.entities.round ?? 1),
    },
    {
      id: "take-back",
      title: "Take a ship back",
      when: aiHere,
      target: ["ship-crew"],
      body: (
        <p>
          The <b>Crew</b> line on a ship&apos;s record says who has her: <b>AI</b> or <b>People</b>. Taking her back
          stops the AI at once; you give her orders from then on.
        </p>
      ),
      task: (
        <>
          Choose <em>Kite</em> in the Ship menu, and press <b>Take back</b>.
        </>
      ),
      done: (now) => findShipIn(now.entities, "Kite")?.controller !== "AI",
    },
    {
      id: "hand-over",
      title: "And hand her over",
      when: aiHere,
      target: ["ship-crew"],
      body: <p>It works the other way too, if a player has to leave mid-game.</p>,
      task: (
        <>
          Press <b>Switch to AI</b>.
        </>
      ),
      done: (now, start) =>
        findShipIn(start.entities, "Kite")?.controller !== "AI" && findShipIn(now.entities, "Kite")?.controller === "AI",
    },
    {
      id: "fleet-command",
      title: "Fleet Command",
      target: ["seat-menu"],
      body: (
        <p>
          <b>Fleet Command</b> is a seat for one player to run a whole side: every station of every ship on it, and
          nothing of the other side&apos;s that their crews have not found.
        </p>
      ),
      task: (
        <>
          Choose <b>Fleet Command (Blue)</b> in the seat menu.
        </>
      ),
      done: (now) => now.user.fleetTeam === "Blue",
    },
    {
      id: "commanding",
      title: "Commanding",
      target: ["ship-menu", "next-round"],
      body: (
        <p>
          The Ship menu now lists only Blue&apos;s ships; open each in turn to give its orders. Like any player,
          you press <b>Ready</b> when they are in, rather than ending the round yourself. A side can have a fleet
          commander and crews on its ships at the same time.
        </p>
      ),
    },
    {
      id: "back",
      title: "Back to GM",
      target: ["seat-menu"],
      body: <p>That is the lesson&apos;s last stop.</p>,
      task: (
        <>
          Choose <b>GM</b> in the seat menu.
        </>
      ),
      done: (now, start) => start.user.fleetTeam != null && now.user.fleetTeam == null,
    },
    {
      id: "done",
      title: "Your fleet",
      body: (
        <p>
          You have flown, fought, been hurt, jumped, and played a scenario. The rest is practice: start a game from
          the scenario screen, perhaps against an AI side. Last of all: making scenarios of your own.
        </p>
      ),
    },
  ],
};
