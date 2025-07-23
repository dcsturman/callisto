import * as React from "react";
import {useEffect, useMemo} from "react";
import * as THREE from "three";
import {Accordion} from "lib/Accordion";
import {AddShip} from "./AddShip";
import {POSITION_SCALE, SCALE} from "lib/universal";
import {Ship, Entity, Planet, findShip} from "lib/entities";
import {ViewMode} from "lib/view";
import {nextRound} from "lib/serverManager";
import {EntitySelector, EntitySelectorType} from "lib/EntitySelector";
import {scaleVector, vectorToString} from "lib/Util";
import {NavigationPlan} from "./ShipComputer";
import {FireActions, FireControl} from "./WeaponUse";
import {ShipComputer} from "./ShipComputer";
import {computeFlightPath} from "lib/serverManager";
import {useAppSelector, useAppDispatch} from "state/hooks";
import {entitiesSelector} from "state/serverSlice";
import {
  setComputerShipName,
  setShowRange,
  setCameraPos,
  setGravityWells,
  setJumpDistance,
} from "state/uiSlice";

function ShipList(args: {camera: THREE.Camera | null}) {
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);
  const dispatch = useAppDispatch();

  const computerShip = useMemo(() => {
    return findShip(entities, computerShipName);
  }, [computerShipName, entities]);

  const choiceHandler = (ship: Entity | null) => {
    dispatch(setShowRange(null));
    dispatch(setComputerShipName(ship ? ship.name : null));
  };

  return (
    <div className="control-launch-div">
      <h2 className="ship-list-label">Ship: </h2>
      <EntitySelector
        id="ship-list-dropdown"
        filter={[EntitySelectorType.Ship]}
        setChoice={choiceHandler}
        current={computerShip}
      />
      <GoButton camera={args.camera} />
    </div>
  );
}

interface GoButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  camera: THREE.Camera | null;
}

export const GoButton: React.FC<GoButtonProps> = ({camera, ...props}) => {
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);
  const dispatch = useAppDispatch();

  const computerShip = useMemo(() => {
    return findShip(entities, computerShipName);
  }, [computerShipName, entities]);

  return (
    <button
      className="control-input blue-button"
      {...props}
      onClick={() => moveCameraToShip(camera, computerShip, (pos) => dispatch(setCameraPos(pos)))}>
      Go
    </button>
  );
};

function moveCameraToShip(
  camera: THREE.Camera | null,
  computerShip: Ship | null,
  setCameraPos: (state: {x: number, y: number, z:number}) => void
) {
  if (camera == null) {
    console.log("Cannot move camera because camera object in Three is null.");
    return;
  }
  if (computerShip) {
    const downCamera = new THREE.Vector3(0, 0, 40);
    downCamera.applyQuaternion(camera.quaternion);
    const new_camera_pos = new THREE.Vector3(
      computerShip.position[0] * SCALE,
      computerShip.position[1] * SCALE,
      computerShip.position[2] * SCALE
    ).add(downCamera);
    console.log("moveCameraToShip: new_camera_pos: " + JSON.stringify(new_camera_pos));
    camera.position.set(new_camera_pos.x, new_camera_pos.y, new_camera_pos.z);
    setCameraPos({x: new_camera_pos.x, y: new_camera_pos.y, z: new_camera_pos.z});
  } else {
    console.log("Cannot move camera because no ship is selected.");
  }
}

export function Controls(args: {
  camera: THREE.Camera | null;
}) {
  const shipName = useAppSelector((state) => state.user.shipName);
  const role = useAppSelector((state) => state.user.role);

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
    const computerShipDesign = computerShip ? shipTemplates[computerShip.design] : null;
    return [computerShip, computerShipDesign];
  }, [computerShipName, entities, shipTemplates]);

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
      <Accordion id="ship-computer" title="Ship's Computer" initialOpen={true}>
        {shipName == null ? (
          <ShipList camera={args.camera} />
        ) : (
          <GoButton
            camera={args.camera}
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
                <pre className="plan-accel-text">{`${computerShip.current_maneuver}(${computerShipDesign.maneuver})`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Jmp</h2>
                <pre className="plan-accel-text">{`${computerShip.current_jump}(${computerShipDesign.jump})`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Power</h2>
                <pre className="plan-accel-text">{`${computerShip.current_power}(${computerShipDesign.power})`}</pre>
              </div>
              {!computerShipDesign.countermeasures && !computerShipDesign.stealth && (
                <div className="stats-bloc-entry">
                  <h2>Sensors</h2>
                  <pre className="plan-accel-text">{computerShip.current_sensors}</pre>
                </div>
              )}
            </div>
            {(computerShipDesign.countermeasures || computerShipDesign.stealth) && (
              <div className="vital-stats-bloc">
                <div className="stats-bloc-entry">
                  <h2>Sensors</h2>
                  <pre className="plan-accel-text">{computerShip.current_sensors}</pre>
                </div>
                <div className="stats-bloc-entry">
                  <h2>CM</h2>
                  <pre className="plan-accel-text">
                    {computerShipDesign.countermeasures || "None"}
                  </pre>
                </div>
                <div className="stats-bloc-entry">
                  <h2>Stealth</h2>
                  <pre className="plan-accel-text">{computerShipDesign.stealth || "None"}</pre>
                </div>
              </div>
            )}
            <h2 className="control-form">Current Position (km)</h2>
            <div style={{display: "flex", justifyContent: "space-between"}}>
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
            <div style={{display: "flex"}}>
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
            <hr />
            {[ViewMode.Pilot, ViewMode.Sensors].includes(role) && computerShipName && (
              <Accordion
                title={`${computerShipName} ${ViewMode[role]} Controls`}
                initialOpen={true}>
                <ShipComputer
                  ship={computerShip}
                  sensorLocks={entities.ships.reduce((acc, ship) => {
                    if (ship.sensor_locks.includes(computerShipName)) {
                      acc.push(ship.name);
                    }
                    return acc;
                  }, [] as string[])}
                />
              </Accordion>
            )}
            {[ViewMode.Gunner, ViewMode.General].includes(role) && (
              <div className="control-form">
                <Accordion title={`${computerShipName} Fire Controls`} initialOpen={true}>
                  <FireControl />
                </Accordion>
              </div>
            )}
          </>
        )}
        {computerShip &&
          computerShipName &&
          computerShipDesign &&
          actions[computerShipName]?.fire?.length +
            actions[computerShipName]?.pointDefense?.length >
            0 && (
            <FireActions
              fireActions={actions[computerShipName].fire || []}
              pointDefenseActions={actions[computerShipName].pointDefense || []}
              design={computerShipDesign}
            />
          )}
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
        }}>
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
      <label style={{display: "flex"}}>
        {" "}
        <input
          type="checkbox"
          checked={gravityWells}
          onChange={() => dispatch(setGravityWells(!gravityWells))}
        />{" "}
        Gravity Well
      </label>
      <label style={{display: "flex"}}>
        {" "}
        <input
          type="checkbox"
          checked={jumpDistance}
          onChange={() => dispatch(setJumpDistance(!jumpDistance)) }
        />{" "}
        100 Diameter Limit
      </label>
    </div>
  );
}
export function EntityInfoWindow(args: {entity: Entity}) {
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
        <p>Position (km): {vectorToString(scaleVector(args.entity.position, 1e-3))}</p>
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
