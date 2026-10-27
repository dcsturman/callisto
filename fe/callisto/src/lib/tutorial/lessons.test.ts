import { describe, it, expect } from "vitest";
import * as fs from "fs";
import * as path from "path";
import { EntityList } from "lib/entities";
import { TutorialContext } from "lib/tutorial/lesson";
import { LESSONS } from "lib/tutorial/lessons";
import { sideView } from "lib/bearing";
import { isTutorialScenario } from "components/scenarios/ScenarioManager";
import uiReducer from "state/uiSlice";
import userReducer from "state/userSlice";

const SRC = path.resolve(__dirname, "../..");
const SCENARIOS = path.resolve(SRC, "../../../callisto/scenarios");

const sourceFiles = (dir: string): string[] =>
  fs.readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(full);
    return /\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name) ? [full] : [];
  });

/** Every `data-tutorial` marker the UI puts on something. */
function markersInUi(): Set<string> {
  const markers = new Set<string>();
  for (const file of sourceFiles(SRC)) {
    const text = fs.readFileSync(file, "utf8");
    for (const match of text.matchAll(/(?:data-tutorial|\btutorial)="([\w-]+)"/g)) markers.add(match[1]);
  }
  // Station cards are marked by their card id: `station-<id>`.
  const stations = fs.readFileSync(path.join(SRC, "components/controls/Stations.tsx"), "utf8");
  const looks = stations.slice(stations.indexOf("CARD_LOOK"), stations.indexOf("};", stations.indexOf("CARD_LOOK")));
  for (const match of looks.matchAll(/^\s+(\w+): \{title/gm)) markers.add(`station-${match[1]}`);
  return markers;
}

/** The game as a lesson's scenario starts it, before the player does anything. */
function startOf(scenario: string): TutorialContext {
  const file = JSON.parse(fs.readFileSync(path.join(SCENARIOS, scenario), "utf8"));
  const entities = {
    ships: file.ships,
    planets: file.planets,
    missiles: [],
    metadata: file.metadata,
    filename: scenario,
    round: 1,
  } as unknown as EntityList;
  return {
    entities,
    ui: uiReducer(undefined, { type: "" }),
    user: userReducer(undefined, { type: "" }),
    actions: {},
  };
}

describe("the tutorial's lessons", () => {
  it("have unique ids, and unique step ids within each", () => {
    expect(new Set(LESSONS.map((l) => l.id)).size).toBe(LESSONS.length);
    for (const lesson of LESSONS) {
      expect(new Set(lesson.steps.map((s) => s.id)).size, lesson.id).toBe(lesson.steps.length);
    }
  });

  it("each play in a scenario that exists and is kept out of the game list", () => {
    for (const lesson of LESSONS) {
      expect(fs.existsSync(path.join(SCENARIOS, lesson.scenario)), lesson.scenario).toBe(true);
      expect(isTutorialScenario(lesson.scenario), lesson.scenario).toBe(true);
    }
  });

  it("give every task a check, and check only tasks", () => {
    for (const lesson of LESSONS) {
      for (const step of lesson.steps) {
        expect(step.task != null, `${lesson.id}/${step.id}`).toBe(step.done != null);
      }
    }
  });

  // What made the old tutorial rot: it pointed at things the UI had renamed
  // or removed, and silently skipped them.
  it("point only at things the UI marks", () => {
    const markers = markersInUi();
    for (const lesson of LESSONS) {
      for (const step of lesson.steps) {
        for (const target of step.target ?? []) {
          expect(markers.has(target), `${lesson.id}/${step.id} points at "${target}"`).toBe(true);
        }
      }
    }
  });

  it("start with no task already done", () => {
    for (const lesson of LESSONS) {
      const start = startOf(lesson.scenario);
      for (const step of lesson.steps) {
        expect(step.done?.(start, start) ?? false, `${lesson.id}/${step.id}`).toBe(false);
      }
    }
  });
});

describe("the lessons' checks", () => {
  const flying = LESSONS.find((l) => l.id === "flying")!;
  const step = (id: string) => flying.steps.find((s) => s.id === id)!;
  const start = startOf(flying.scenario);

  it("see a choice of ship", () => {
    const now = { ...start, ui: { ...start.ui, computerShipName: "Killer" } };
    expect(step("choose-ship").done!(now, start)).toBe(true);
  });

  it("see a new plan assigned", () => {
    const ships = start.entities.ships.map((s) =>
      s.name === "Killer" ? { ...s, plan: [[[1, 0, 0], 360], null] } : s,
    );
    const now = { ...start, entities: { ...start.entities, ships } as EntityList };
    expect(step("assign").done!(now, start)).toBe(true);
  });

  it("see Killer arrive within 2,000 km of Beowulf, and not before", () => {
    const beowulf = start.entities.ships.find((s) => s.name === "Beowulf")!;
    const near = (km: number) => {
      const ships = start.entities.ships.map((s) =>
        s.name === "Killer"
          ? { ...s, position: [beowulf.position[0] + km * 1000, beowulf.position[1], beowulf.position[2]] }
          : s,
      );
      return { ...start, entities: { ...start.entities, ships } as EntityList };
    };
    expect(step("fly").done!(near(1500), start)).toBe(true);
    expect(step("fly").done!(near(2500), start)).toBe(false);
  });

  it("see the camera put side-on by Side view, either way round", () => {
    const lesson = LESSONS[0];
    const side = lesson.steps.find((s) => s.id === "side-view")!;
    const opening = startOf(lesson.scenario);
    const ship = (name: string) => opening.entities.ships.find((s) => s.name === name)!.position;
    for (const [a, b] of [["Killer", "Beowulf"], ["Beowulf", "Killer"]]) {
      const view = sideView(ship(a), ship(b), 75);
      const now = { ...opening, ui: { ...opening.ui, cameraPos: view.position } };
      expect(side.done!(now, opening)).toBe(true);
    }
  });

  it("count rounds from when the step began", () => {
    const rounds = LESSONS[0].steps.find((s) => s.id === "rounds")!;
    const at = (round: number) => ({ ...start, entities: { ...start.entities, round } as EntityList });
    expect(rounds.done!(at(4), at(3))).toBe(false);
    expect(rounds.done!(at(5), at(3))).toBe(true);
  });
});
