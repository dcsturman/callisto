import * as React from "react";
import { useEffect, useMemo, useCallback, useState } from "react";
import {
  SaveScenarioDialog,
} from "components/scenarios/SaveScenarioDialog";
import { SCENARIO_BUILDER_PREFIX } from "components/scenarios/ScenarioManager";
import * as THREE from "three";
import { Accordion } from "lib/Accordion";
import { AddShip } from "./AddShip";
import { AddPlanet } from "./AddPlanet";
import { EntityList } from "./EntityList";
import { POSITION_SCALE, SCALE } from "lib/universal";
import {
  Ship,
  Entity,
  Planet,
  findShip,
  availablePower,
  stationsDown,
} from "lib/entities";
import { shipWeapons } from "lib/shipDesignTemplates";
import { isReferee } from "lib/view";
import { SectionTag } from "components/controls/SectionTag";
import { FaUsers, FaExclamationTriangle } from "react-icons/fa";
import { ENGINEER_SKILLS } from "components/controls/CrewBuilder";
import { nextRound, setReady, setShipTeam } from "lib/serverManager";
import { Team, TEAMS, teamLabelColor } from "lib/teams";
import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import { scaleVector, vectorToString } from "lib/Util";
import { NavigationPlan } from "./ShipComputer";
import { computeFlightPath } from "lib/serverManager";
import { useAppSelector, useAppDispatch } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";
import { AppMode } from "state/tutorialSlice";
import { store } from "state/store";
import {
  setComputerShipName,
  setShowRange,
  setCameraPos,
  setGravityWells,
  setJumpDistance,
} from "state/uiSlice";

function ShipList(args: {
  moveCamera: (
    cameraQuaternion: [number, number, number, number],
    ship: Ship,
  ) => void;
}) {
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);
  const dispatch = useAppDispatch();

  const computerShip = useMemo(() => {
    return findShip(entities, computerShipName);
  }, [computerShipName, entities]);

  const choiceHandler = useCallback(
    (ship: Entity | null) => {
      dispatch(setShowRange(null));
      dispatch(setComputerShipName(ship ? ship.name : null));
    },
    [dispatch],
  );

  const filter = useMemo(() => [EntitySelectorType.Ship], []);

  return (
    <div className="control-launch-div">
      <h2 className="ship-list-label">Ship: </h2>
      <EntitySelector
        id="ship-list-dropdown"
        filter={filter}
        setChoice={choiceHandler}
        current={computerShip}
      />
      <GoButton moveCamera={args.moveCamera} />
    </div>
  );
}

interface GoButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  moveCamera: (
    cameraQuaternion: [number, number, number, number],
    ship: Ship,
  ) => void;
}

export const GoButton: React.FC<GoButtonProps> = ({ moveCamera, ...props }) => {
  const cameraQuaternion = useAppSelector((state) => state.ui.cameraQuaternion);
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);

  const computerShip = useMemo(() => {
    return findShip(entities, computerShipName);
  }, [computerShipName, entities]);

  const clickHandler = useMemo(
    () => () => computerShip && moveCamera(cameraQuaternion, computerShip),
    [computerShip, cameraQuaternion, moveCamera],
  );

  return (
    <button
      className="control-input blue-button"
      {...props}
      onClick={clickHandler}
    >
      Go
    </button>
  );
};

function moveCameraToShip(
  cameraQuaternion: [number, number, number, number],
  computerShip: Ship,
) {
  const downCamera = new THREE.Vector3(0, 0, 40);
  downCamera.applyQuaternion(
    new THREE.Quaternion(
      cameraQuaternion[0],
      cameraQuaternion[1],
      cameraQuaternion[2],
      cameraQuaternion[3],
    ),
  );
  const new_camera_pos = new THREE.Vector3(
    computerShip.position[0] * SCALE,
    computerShip.position[1] * SCALE,
    computerShip.position[2] * SCALE,
  ).add(downCamera);
  store.dispatch(
    setCameraPos({
      x: new_camera_pos.x,
      y: new_camera_pos.y,
      z: new_camera_pos.z,
    }),
  );
}

// Builder-mode panel: AddShip / AddPlanet plus a Save button at the bottom.
// Split out from Controls() so the save dialog state and the various
// joinedScenario-derived defaults only live here.
function ScenarioBuilderControls(args: {
  shipTemplates: Record<string, unknown>;
  entities: {
    metadata?: { name?: string; description?: string };
    filename?: string;
  };
}) {
  const joinedScenario = useAppSelector((state) => state.user.joinedScenario);
  const activeScenarios = useAppSelector((state) => state.server.activeScenarios);
  const scenarioTemplates = useAppSelector(
    (state) => state.server.scenarioTemplates,
  );
  const [saveOpen, setSaveOpen] = useState(false);

  // Look up the template this builder session was loaded from, if any.
  // (filename, metadata) — both useful for prefilling the save dialog.
  const templateLookup = useMemo(() => {
    if (!joinedScenario) return null;
    const active = activeScenarios.find(([id]) => id === joinedScenario);
    if (!active || !active[1]) return null;
    const filename = active[1];
    const tmpl = scenarioTemplates.find(([fn]) => fn === filename);
    if (!tmpl) return null;
    return { filename, metadata: tmpl[1] };
  }, [joinedScenario, activeScenarios, scenarioTemplates]);

  // Filename default — entities.filename if the wire delivered it, else the
  // template filename from the picker tables, else strip the SCENARIO- prefix
  // off the builder session ID, else empty (user-typed-from-scratch). Always
  // strip the .json suffix; the backend re-appends it on save and the user
  // shouldn't have to think about the extension.
  const defaultFilename = useMemo(() => {
    const raw = args.entities.filename
      || templateLookup?.filename
      || (joinedScenario && joinedScenario.startsWith(SCENARIO_BUILDER_PREFIX)
        ? joinedScenario.slice(SCENARIO_BUILDER_PREFIX.length)
        : "");
    return raw.replace(/\.json$/i, "");
  }, [args.entities.filename, templateLookup, joinedScenario]);

  // Display name and description follow the same fallback chain as filename:
  // live entities, then template lookup, then empty. Use truthy checks so
  // empty strings on entities don't short-circuit before the template fallback.
  const defaultDisplayName = useMemo(() => {
    if (args.entities.metadata?.name) return args.entities.metadata.name;
    if (templateLookup?.metadata.name) return templateLookup.metadata.name;
    return "";
  }, [args.entities.metadata?.name, templateLookup]);

  const defaultDescription = useMemo(() => {
    if (args.entities.metadata?.description) return args.entities.metadata.description;
    if (templateLookup?.metadata.description) return templateLookup.metadata.description;
    return "";
  }, [args.entities.metadata?.description, templateLookup]);

  return (
    <div className="controls-pane">
      <h1>Controls</h1>
      <hr />
      {Object.keys(args.shipTemplates).length > 0 && (
        <>
          <AddShip />
          <hr />
        </>
      )}
      <AddPlanet />
      <hr />
      <EntityList />
      <button
        type="button"
        className="control-button blue-button save-scenario-anchor"
        onClick={() => setSaveOpen(true)}
      >
        Save Scenario
      </button>
      {saveOpen && (
        <SaveScenarioDialog
          initialName={defaultFilename}
          initialDisplayName={defaultDisplayName}
          initialDescription={defaultDescription}
          onClose={() => setSaveOpen(false)}
        />
      )}
    </div>
  );
}

/** "3" for one of them, "3/1" for a watch, "none" for an empty station. */
const listSkills = (skills: number[] | undefined): string =>
  skills == null || skills.length === 0 ? "none" : skills.join("/");

export function Controls() {
  const shipName = useAppSelector((state) => state.user.shipName);
  const roles = useAppSelector((state) => state.user.roles);
  // My own entry in the user list, to show whether I have readied up. The
  // server names players by the local part of their email, as it does for
  // everyone else in the list.
  const email = useAppSelector((state) => state.user.email);
  const users = useAppSelector((state) => state.server.users);
  const amReady = useMemo(() => {
    const me = email ? email.split("@")[0] : null;
    return users.some((user) => user.display_name === me && user.ready);
  }, [users, email]);

  const isScenarioBuilder = useAppSelector(
    (state) => state.tutorial.appMode === AppMode.ScenarioBuilder,
  );

  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);
  const shipTemplates = useAppSelector((state) => state.server.templates);
  const showRange = useAppSelector((state) => state.ui.showRange);

  const dispatch = useAppDispatch();

  // If there's actually a ship name defined in the Role information, that supersedes
  // any other selection for the computerShip.
  useEffect(() => {
    if (shipName) {
      dispatch(setComputerShipName(shipName));
    }
  }, [shipName, dispatch]);

  const [computerShip, computerShipDesign] = useMemo(() => {
    const computerShip = findShip(entities, computerShipName);
    const computerShipDesign = computerShip
      ? shipTemplates[computerShip.design]
      : null;
    return [computerShip, computerShipDesign];
  }, [computerShipName, entities, shipTemplates]);

  if (isScenarioBuilder) {
    return <ScenarioBuilderControls shipTemplates={shipTemplates} entities={entities} />;
  }

  return (
    <div className="controls-pane">
      <h1>Controls</h1>
      <hr />
      {/* Referee only. General mode with a ship assigned is a player flying
          that ship with every station open, not the GM; that is the case that
          was leaking Add Ship. Same condition App.tsx uses for the reset. */}
      {isReferee(roles, shipName) && Object.keys(shipTemplates).length > 0 && (
        <>
          <AddShip />
          <hr />
        </>
      )}
      <Accordion id="ship-computer" title="Ship's Computer" initialOpen={true}>
        {shipName == null ? (
          <ShipList moveCamera={moveCameraToShip} />
        ) : (
          <GoButton
            moveCamera={moveCameraToShip}
            style={{
              width: "100%",
              height: "24px",
              margin: "0px",
              padding: "0px",
            }}
          />
        )}
        {computerShip && computerShipDesign && (
          <>
            <div className="vital-stats-bloc">
              <div className="stats-bloc-entry">
                <h2>Design</h2>
                <pre className="plan-accel-text">{computerShip.design}</pre>
              </div>
              {/* Which side the ship is on belongs to the ship, not to any one
                  crew station, so it sits with the ship's other numbers where
                  every role can see and set it. */}
              <div className="stats-bloc-entry">
                <h2>Team</h2>
                <select
                  className="team-select"
                  value={computerShip.team ?? ""}
                  style={{ color: teamLabelColor(computerShip.team, {}) }}
                  title="Which side this ship is on. Teams are colour-coded in the view, always know where each other are, and will not fire on one another."
                  onChange={(event) =>
                    setShipTeam(
                      computerShip.name,
                      (event.target.value || null) as Team | null,
                    )
                  }
                >
                  <option value="">Unaligned</option>
                  {TEAMS.map((team) => (
                    <option key={team} value={team}>
                      {team}
                    </option>
                  ))}
                </select>
              </div>
              <div className="stats-bloc-entry">
                <h2>Hull</h2>
                <pre className="plan-accel-text">{`${computerShip.current_hull}(${computerShipDesign.hull})`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Armor</h2>
                <pre className="plan-accel-text">{`${computerShip.current_armor}(${computerShipDesign.armor})`}</pre>
              </div>
            </div>
            <div className="vital-stats-bloc">
              <div className="stats-bloc-entry">
                <h2>Man</h2>
                <pre className="plan-accel-text">{`${computerShip.current_maneuver}(${computerShipDesign.maneuver + (computerShip.temporary_maneuver ?? 0)})`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Jmp</h2>
                <pre className="plan-accel-text">{`${computerShip.current_jump}(${computerShipDesign.jump})`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Power</h2>
                <pre className="plan-accel-text">{`${availablePower(computerShip)}(${computerShipDesign.power})`}</pre>
              </div>
              {!computerShipDesign.countermeasures &&
                !computerShipDesign.stealth && (
                  <div className="stats-bloc-entry">
                    <h2>Sensors</h2>
                    <pre className="plan-accel-text">
                      {computerShip.current_sensors}
                    </pre>
                  </div>
                )}
            </div>
            {(computerShipDesign.countermeasures ||
              computerShipDesign.stealth) && (
              <div className="vital-stats-bloc">
                <div className="stats-bloc-entry">
                  <h2>Sensors</h2>
                  <pre className="plan-accel-text">
                    {computerShip.current_sensors}
                  </pre>
                </div>
                <div className="stats-bloc-entry">
                  <h2>CM</h2>
                  <pre className="plan-accel-text">
                    {computerShipDesign.countermeasures || "None"}
                  </pre>
                </div>
                <div className="stats-bloc-entry">
                  <h2>Stealth</h2>
                  <pre className="plan-accel-text">
                    {computerShipDesign.stealth || "None"}
                  </pre>
                </div>
              </div>
            )}
            <h2 className="control-form">Current Position (km)</h2>
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <pre className="plan-accel-text">
                {"(" +
                  (computerShip.position[0] / POSITION_SCALE).toFixed(0) +
                  ", " +
                  (computerShip.position[1] / POSITION_SCALE).toFixed(0) +
                  ", " +
                  (computerShip.position[2] / POSITION_SCALE).toFixed(0) +
                  ")"}
              </pre>
              <span>
                <input
                  id="show-range-checkbox"
                  type="checkbox"
                  // Checked only when the circles are around THIS ship. It
                  // used to read as "on" for any ship once toggled anywhere,
                  // so switching ships left the box ticked while the circles
                  // stayed around the previous one. Toggling on a different
                  // ship now re-targets rather than clears.
                  checked={showRange === computerShipName}
                  onChange={() => {
                    if (showRange !== computerShipName && computerShipName) {
                      dispatch(setShowRange(computerShipName));
                    } else {
                      dispatch(setShowRange(null));
                    }
                  }}
                />
                &nbsp;Ranges
              </span>
            </div>
            <h2 className="control-form">Current Velocity (m/s)</h2>
            <div style={{ display: "flex" }}>
              <pre className="plan-accel-text">
                {"(" +
                  computerShip.velocity[0].toFixed(0) +
                  ", " +
                  computerShip.velocity[1].toFixed(0) +
                  ", " +
                  computerShip.velocity[2].toFixed(0) +
                  ")"}
              </pre>
            </div>
            <div id="current-plan-heading">
              <h2 className="control-form">Current Plan (s @ G&apos;s)</h2>
              <NavigationPlan plan={computerShip.plan} />
            </div>
            {((computerShip.crit_level &&
              computerShip.crit_level.some((c) => c > 0)) ||
              stationsDown(computerShip).length > 0) && (
                <div id="crits-display">
                  <SectionTag icon={<FaExclamationTriangle />}>Critical Hits</SectionTag>
                  <pre className="plan-accel-text">
                    {(() => {
                      const systems = [
                        "Sensors",
                        "Power",
                        "Fuel",
                        "Weapon",
                        "Armor",
                        "Hull",
                        "Maneuver",
                        "Cargo",
                        "Jump",
                        "Crew",
                        "Bridge",
                      ];
                      const crits = (computerShip.crit_level ?? [])
                        .map((level, index) => {
                          if (level === 0) return null;
                          return `${systems[index]}: ${level}`;
                        })
                        .filter(Boolean);

                      // Group into rows of 3
                      const rows = [];
                      for (let i = 0; i < crits.length; i += 3) {
                        rows.push(crits.slice(i, i + 3).join(", "));
                      }
                      // Stations that are out get their own line, since
                      // they are what stops the crew doing something.
                      const down = stationsDown(computerShip);
                      if (down.length > 0) {
                        rows.push(`Bridge stations: ${down.join(", ")}`);
                      }
                      return rows.join("\n");
                    })()}
                  </pre>
                </div>
              )}
            {/* Bottom of the box, one heading in the section-title style and
                one line in the body font. Gunners are one number per mount,
                in mount order. */}
            <SectionTag icon={<FaUsers />}>Crew</SectionTag>
            {/* Sensor operators and engineers are listed one per person, in
                the order they sit in the crew, since the dropdowns that put
                one of them on a station name them by that position. */}
            <p className="crew-line">
              {[
                `Pilot - ${computerShip.crew.pilot}`,
                `Sensors - ${listSkills(computerShip.crew.sensors)}`,
                ...ENGINEER_SKILLS.map(
                  (skill) =>
                    `${skill.label} - ${listSkills(
                      (computerShip.crew.engineers ?? []).map((engineer) => engineer[skill.key] ?? 0),
                    )}`,
                ),
                `Leadership - ${computerShip.crew.leadership ?? 0}`,
                `Gunners - ${shipWeapons(computerShip, shipTemplates)
                  .map((_w, i) => computerShip.crew.gunnery[i] ?? 0)
                  .join(", ") || "none"}`,
              ].join(",  ")}
            </p>
          </>
        )}
      </Accordion>
      {/* The referee ends the round; everyone else says when their orders are
          in. Two people pressing Next Round ends the round before the rest of
          the table has finished giving theirs. */}
      {isReferee(roles, shipName) ? (
        <button
          className="control-input control-button blue-button button-next-round"
          // Reset the computer and route on the next round.  If this gets any more complex move it into its
          // own function.
          onClick={() => {
            computeFlightPath(null, [0, 0, 0], [0, 0, 0], null, null, 0);
            // Strip out the details on the weapons and provide an object with just
            // the name of each possible actor and the FireState they produced during the round.
            nextRound();
            //args.setComputerShip(null);
          }}
        >
          Next Round
        </button>
      ) : (
        <button
          className="control-input control-button blue-button button-next-round"
          title={
            amReady
              ? "Take it back if you still have orders to give"
              : "Tell the GM your orders are in"
          }
          onClick={() => setReady(!amReady)}
        >
          {amReady ? "Not Ready" : "Ready"}
        </button>
      )}
    </div>
  );
}

export function ViewControls() {
  const gravityWells = useAppSelector((state) => state.ui.gravityWells);
  const jumpDistance = useAppSelector((state) => state.ui.jumpDistance);
  const dispatch = useAppDispatch();

  return (
    <div className="view-controls-window">
      <h2>View Controls</h2>
      <label style={{ display: "flex" }}>
        {" "}
        <input
          type="checkbox"
          checked={gravityWells}
          onChange={() => dispatch(setGravityWells(!gravityWells))}
        />{" "}
        Gravity Well
      </label>
      <label style={{ display: "flex" }}>
        {" "}
        <input
          type="checkbox"
          checked={jumpDistance}
          onChange={() => dispatch(setJumpDistance(!jumpDistance))}
        />{" "}
        100 Diameter Limit
      </label>
      {/* Collapsed by default: this is reference material, not a control, and
          the top-right corner has no room to spare. It exists at all because
          the flight keys were documented nowhere -- not in the tutorial, not
          in the README -- so the only way to find them was to read
          `FlyControls`. */}
      <Accordion
        className="camera-keys"
        title="Camera Keys"
        initialOpen={false}>
        <dl className="camera-key-list">
          <dt>
            <kbd>W</kbd> <kbd>S</kbd>
          </dt>
          <dd>forward / back</dd>
          <dt>
            <kbd>A</kbd> <kbd>D</kbd>
          </dt>
          <dd>left / right</dd>
          <dt>
            <kbd>R</kbd> <kbd>F</kbd>
          </dt>
          <dd>up / down</dd>
          <dt>
            <kbd>Q</kbd> <kbd>E</kbd>
          </dt>
          <dd>roll</dd>
          <dt>
            <kbd>&uarr;</kbd> <kbd>&darr;</kbd> <kbd>&larr;</kbd> <kbd>&rarr;</kbd>
          </dt>
          <dd>pitch / yaw</dd>
          <dt>
            <kbd>Shift</kbd>
          </dt>
          <dd>faster</dd>
          <dt>drag</dt>
          <dd>look</dd>
        </dl>
      </Accordion>
    </div>
  );
}
export function EntityInfoWindow(args: { entity: Entity }) {
  let isPlanet = false;
  let isShip = false;
  let ship_next_accel: [number, number, number] = [0, 0, 0];
  let radiusKm = 0;
  let design = "";

  // Test if its a Planet
  if ("radius" in args.entity) {
    isPlanet = true;
    radiusKm = (args.entity as Planet).radius / 1000.0;
  } else if ("plan" in args.entity) {
    // If its a Ship
    isShip = true;
    ship_next_accel = (args.entity as Ship).plan[0][0];
    design = "(" + (args.entity as Ship).design + " class)";
  }

  return (
    <div id="ship-info-window" className="ship-info-window">
      <h2 className="ship-info-title">{args.entity.name + " " + design}</h2>
      <div className="ship-info-content">
        <p>
          Position (km):{" "}
          {vectorToString(scaleVector(args.entity.position, 1e-3))}
        </p>
        <p>Velocity (m/s): {vectorToString(args.entity.velocity)}</p>
        {isPlanet ? (
          <p>Radius (km): {radiusKm}</p>
        ) : isShip ? (
          <p> Acceleration (G): {vectorToString(ship_next_accel, 2)}</p>
        ) : (
          <></>
        )}
      </div>
    </div>
  );
}

export default Controls;
