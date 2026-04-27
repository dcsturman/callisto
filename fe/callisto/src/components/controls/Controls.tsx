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
import { POSITION_SCALE, SCALE } from "lib/universal";
import { Ship, Entity, Planet, findShip } from "lib/entities";
import { ViewMode } from "lib/view";
import { nextRound } from "lib/serverManager";
import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import { scaleVector, vectorToString } from "lib/Util";
import { NavigationPlan } from "./ShipComputer";
import { Actions, FireControl } from "./WeaponUse";
import { DEFAULT_SENSOR_STATE, SensorAction } from "components/controls/Actions";
import { ShipComputer } from "./ShipComputer";
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

export function Controls() {
  const shipName = useAppSelector((state) => state.user.shipName);
  const role = useAppSelector((state) => state.user.role);
  const isScenarioBuilder = useAppSelector(
    (state) => state.tutorial.appMode === AppMode.ScenarioBuilder,
  );

  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);
  const shipTemplates = useAppSelector((state) => state.server.templates);
  const actions = useAppSelector((state) => state.actions);
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
      {role === ViewMode.General && Object.keys(shipTemplates).length > 0 && (
        <>
          <AddShip />
          <hr />
        </>
      )}
      {true && (
        <>
          <AddPlanet />
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
                <pre className="plan-accel-text">{`${computerShip.current_power}(${computerShipDesign.power})`}</pre>
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
                  checked={showRange !== null}
                  onChange={() => {
                    if (showRange === null && computerShipName) {
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
            {computerShip.crit_level &&
              computerShip.crit_level.some((c) => c > 0) && (
                <div id="crits-display">
                  <h2 className="control-form">Critical Hits</h2>
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
                      const crits = computerShip.crit_level
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
                      return rows.join("\n");
                    })()}
                  </pre>
                </div>
              )}
            <hr />
            {[ViewMode.Pilot, ViewMode.Sensors, ViewMode.Engineer].includes(
              role,
            ) &&
              computerShipName && (
                <Accordion
                  title={`${computerShipName} ${ViewMode[role]} Controls`}
                  initialOpen={true}
                >
                  <ShipComputer ship={computerShip} />
                </Accordion>
              )}
            {[ViewMode.Gunner, ViewMode.General].includes(role) && (
              <div className="control-form">
                <Accordion
                  title={`${computerShipName} Fire Controls`}
                  initialOpen={true}
                >
                  <FireControl />
                </Accordion>
              </div>
            )}
          </>
        )}
        {computerShip && computerShipName && computerShipDesign && (() => {
          const a = actions[computerShipName];
          if (!a) return null;
          // Per-role visibility. General sees everything; specialist roles see
          // only their own action category. Pilot / Observer see nothing here.
          const seeFire = role === ViewMode.General || role === ViewMode.Gunner;
          const seeSensor = role === ViewMode.General || role === ViewMode.Sensors;
          const seeEngineer = role === ViewMode.General || role === ViewMode.Engineer;
          const fireActions = seeFire ? a.fire || [] : [];
          const pdActions = seeFire ? a.pointDefense || [] : [];
          const sensorAction = seeSensor
            ? a.sensor || DEFAULT_SENSOR_STATE
            : DEFAULT_SENSOR_STATE;
          const engineerAction = seeEngineer ? a.engineer ?? null : null;
          const hasAny =
            fireActions.length > 0 ||
            pdActions.length > 0 ||
            sensorAction.action !== SensorAction.None ||
            engineerAction != null;
          if (!hasAny) return null;
          return (
            <Actions
              fireActions={fireActions}
              pointDefenseActions={pdActions}
              sensorAction={sensorAction}
              engineerAction={engineerAction}
              design={computerShipDesign}
            />
          );
        })()}
      </Accordion>
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
