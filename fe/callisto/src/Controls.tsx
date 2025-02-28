import React, { useContext, useEffect } from "react";
import * as THREE from "three";
import { Accordion } from "./Accordion";
import { AddShip } from "./AddShip";
import { ActionContext } from "./Actions";
import {
  Ship,
  ViewControlParams,
  Entity,
  EntitiesServerContext,
  FlightPathResult,
  Planet,
  ShipDesignTemplates,
  ViewContext,
  ViewMode,
  POSITION_SCALE,
  SCALE,
} from "./Universal";

import { addShip, nextRound } from "./ServerManager";
import { EntitySelector, EntitySelectorType } from "./EntitySelector";
import {
  scaleVector,
  vectorToString,
} from "./Util";
import { NavigationPlan } from "./ShipComputer";
import { FireActions, FireControl } from "./WeaponUse";
import { ShipComputer } from "./ShipComputer";

function ShipList(args: {
  computerShip: Ship | null;
  setComputerShip: (ship: Ship | null) => void;
  setCameraPos: (pos: THREE.Vector3) => void;
  camera: THREE.Camera | null;
}) {
  return (
    <div className="control-launch-div">
      <h2 className="ship-list-label">Ship: </h2>
      <EntitySelector
        filter={[EntitySelectorType.Ship]}
        setChoice={(entity) => args.setComputerShip(entity as Ship)}
        current={args.computerShip}
      />
      <GoButton
        camera={args.camera}
        computerShip={args.computerShip}
        setCameraPos={args.setCameraPos}
      />
    </div>
  );
}

interface GoButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  camera: THREE.Camera | null;
  computerShip: Ship | null;
  setCameraPos: (pos: THREE.Vector3) => void;
}

export const GoButton: React.FC<GoButtonProps> = ({
  camera,
  computerShip,
  setCameraPos,
  ...props
}) => {
  return (
    <button
      className="control-input blue-button"
      {...props}
      onClick={() => moveCameraToShip(camera, computerShip, setCameraPos)}>
      Go
    </button>
  );
};

function moveCameraToShip(
  camera: THREE.Camera | null,
  computerShip: Ship | null,
  setCameraPos: (pos: THREE.Vector3) => void
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
    setCameraPos(new_camera_pos);
  } else {
    console.log("Cannot move camera because no ship is selected.");
  }
}

export function Controls(args: {
  computerShip: Ship | null;
  setComputerShip: (ship: Ship | null) => void;
  shipDesignTemplates: ShipDesignTemplates;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null,
    standoff: number
  ) => void;
  setCameraPos: (pos: THREE.Vector3) => void;
  camera: THREE.Camera | null;
  setAuthenticated: (authenticated: boolean) => void;
  showRange: string | null;
  setShowRange: (target: string | null) => void;
  proposedPlan: FlightPathResult | null;
  resetProposedPlan: () => void;
}) {
  const actionContext = useContext(ActionContext);
  const viewContext = useContext(ViewContext);
  const serverEntities = useContext(EntitiesServerContext).entities;

  // If there's actually a ship name defined in the Role information, that supersedes
  // any other selection for the computerShip.
  useEffect(() => {
    if (viewContext.shipName) {
      args.setComputerShip(
        serverEntities.ships.find((s) => s.name === viewContext.shipName) ??
          null
      );
    }
  }, [viewContext.shipName, serverEntities.ships, args]);

  const computerShipDesign = args.computerShip
    ? args.shipDesignTemplates[args.computerShip.design]
    : null;

  return (
    <div className="controls-pane">
      <h1>Controls</h1>
      <hr />
      {viewContext.role === ViewMode.General &&
        args.shipDesignTemplates &&
        Object.keys(args.shipDesignTemplates).length > 0 && (
          <>
            <AddShip
              submitHandler={(ship: Ship) => addShip(ship)}
              shipDesignTemplates={args.shipDesignTemplates}
            />
            <hr />
          </>
        )}
      <Accordion id="ship-computer" title="Ship's Computer" initialOpen={true}>
        {viewContext.shipName === null ? (
          <ShipList
            computerShip={args.computerShip}
            setComputerShip={(ship) => {
              args.setShowRange(null);
              args.setComputerShip(ship);
            }}
            setCameraPos={args.setCameraPos}
            camera={args.camera}
          />
        ) : (
          <GoButton
            camera={args.camera}
            computerShip={args.computerShip}
            setCameraPos={args.setCameraPos}
            style={{
              width: "100%",
              height: "24px",
              margin: "0px",
              padding: "0px",
            }}
          />
        )}
        {args.computerShip && (
          <>
            <div className="vital-stats-bloc">
              <div className="stats-bloc-entry">
                <h2>Design</h2>
                <pre className="plan-accel-text">
                  {args.computerShip.design}
                </pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Hull</h2>
                <pre className="plan-accel-text">{`${
                  args.computerShip.current_hull
                }(${
                  args.shipDesignTemplates[args.computerShip.design].hull
                })`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Armor</h2>
                <pre className="plan-accel-text">{`${
                  args.computerShip.current_armor
                }(${
                  args.shipDesignTemplates[args.computerShip.design].armor
                })`}</pre>
              </div>
            </div>
            <div className="vital-stats-bloc">
              <div className="stats-bloc-entry">
                <h2>Man</h2>
                <pre className="plan-accel-text">{`${
                  args.computerShip.current_maneuver
                }(${
                  args.shipDesignTemplates[args.computerShip.design].maneuver
                })`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Jmp</h2>
                <pre className="plan-accel-text">{`${
                  args.computerShip.current_jump
                }(${
                  args.shipDesignTemplates[args.computerShip.design].jump
                })`}</pre>
              </div>
              <div className="stats-bloc-entry">
                <h2>Power</h2>
                <pre className="plan-accel-text">{`${
                  args.computerShip.current_power
                }(${
                  args.shipDesignTemplates[args.computerShip.design].power
                })`}</pre>
              </div>
              {!args.shipDesignTemplates[args.computerShip.design]
                .countermeasures &&
                !args.shipDesignTemplates[args.computerShip.design].stealth && (
                  <div className="stats-bloc-entry">
                    <h2>Sensors</h2>
                    <pre className="plan-accel-text">
                      {args.computerShip.current_sensors}
                    </pre>
                  </div>
                )}
            </div>
            {(args.shipDesignTemplates[args.computerShip.design]
              .countermeasures ||
              args.shipDesignTemplates[args.computerShip.design].stealth) && (
              <div className="vital-stats-bloc">
                <div className="stats-bloc-entry">
                  <h2>Sensors</h2>
                  <pre className="plan-accel-text">
                    {args.computerShip.current_sensors}
                  </pre>
                </div>
                <div className="stats-bloc-entry">
                  <h2>CM</h2>
                  <pre className="plan-accel-text">
                    {args.shipDesignTemplates[args.computerShip.design]
                      .countermeasures || "None"}
                  </pre>
                </div>
                <div className="stats-bloc-entry">
                  <h2>Stealth</h2>
                  <pre className="plan-accel-text">
                    {args.shipDesignTemplates[args.computerShip.design]
                      .stealth || "None"}
                  </pre>
                </div>
              </div>
            )}
            <h2 className="control-form">Current Position</h2>
            <div style={{ display: "flex", justifyContent: "space-around" }}>
              <pre className="plan-accel-text">
                {"(" +
                  (args.computerShip.position[0] / POSITION_SCALE).toFixed(0) +
                  ", " +
                  (args.computerShip.position[1] / POSITION_SCALE).toFixed(0) +
                  ", " +
                  (args.computerShip.position[2] / POSITION_SCALE).toFixed(0) +
                  ")"}
              </pre>
              <span>
                <input
                  type="checkbox"
                  checked={args.showRange !== null}
                  onChange={() => {
                    if (args.showRange === null && args.computerShip) {
                      args.setShowRange(args.computerShip.name);
                    } else {
                      args.setShowRange(null);
                    }
                  }}
                />
                &nbsp;Ranges
              </span>
            </div>
            <h2 className="control-form">
              Current Plan (s @ m/s<sup>2</sup>)
            </h2>
            <NavigationPlan plan={args.computerShip.plan} />
            <hr />
            {[ViewMode.Pilot, ViewMode.Sensors].includes(viewContext.role) &&
              args.computerShip && (
                <Accordion
                  title={`${args.computerShip.name} ${
                    ViewMode[viewContext.role]
                  } Controls`}
                  initialOpen={true}>
                  <ShipComputer
                    ship={args.computerShip}
                    setComputerShip={args.setComputerShip}
                    proposedPlan={args.proposedPlan}
                    resetProposedPlan={args.resetProposedPlan}
                    getAndShowPlan={args.getAndShowPlan}
                    sensorLocks={serverEntities.ships.reduce((acc, ship) => {
                      if (ship.sensor_locks.includes(args.computerShip!.name)) {
                        acc.push(ship.name);
                      }
                      return acc;
                    }, [] as string[])}
                  />
                </Accordion>
              )}
            {[ViewMode.Gunner, ViewMode.General].includes(viewContext.role) && (
              <div className="control-form">
                <Accordion
                  title={`${args.computerShip.name} Fire Controls`}
                  initialOpen={true}>
                  <FireControl computerShip={args.computerShip} />
                </Accordion>
              </div>
            )}
          </>
        )}
        {args.computerShip &&
          computerShipDesign &&
          (actionContext.actions[args.computerShip.name]?.fire || []).length > 0 && (
            <FireActions
              actions={actionContext.actions[args.computerShip?.name].fire || []}
              design={computerShipDesign}
              computerShipName={args.computerShip.name}
            />
          )}
      </Accordion>
      <button
        className="control-input control-button blue-button button-next-round"
        // Reset the computer and route on the next round.  If this gets any more complex move it into its
        // own function.
        onClick={() => {
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, 0);
          // Strip out the details on the weapons and provide an object with just
          // the name of each possible actor and the FireState they produced during the round.
          nextRound();
          args.setShowRange(null);
          //args.setComputerShip(null);
        }}>
        Next Round
      </button>
    </div>
  );
}

export function ViewControls(args: {
  setViewControls: (controls: ViewControlParams) => void;
  viewControls: ViewControlParams;
}) {
  return (
    <div className="view-controls-window">
      <h2>View Controls</h2>
      <label style={{ display: "flex" }}>
        {" "}
        <input
          type="checkbox"
          checked={args.viewControls.gravityWells}
          onChange={() =>
            args.setViewControls({
              ...args.viewControls,
              gravityWells: !args.viewControls.gravityWells,
            })
          }
        />{" "}
        Gravity Well
      </label>
      <label style={{ display: "flex" }}>
        {" "}
        <input
          type="checkbox"
          checked={args.viewControls.jumpDistance}
          onChange={() =>
            args.setViewControls({
              ...args.viewControls,
              jumpDistance: !args.viewControls.jumpDistance,
            })
          }
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

  if (args.entity instanceof Planet) {
    isPlanet = true;
    radiusKm = args.entity.radius / 1000.0;
  } else if (args.entity instanceof Ship) {
    isShip = true;
    ship_next_accel = args.entity.plan[0][0];
    design = "(" + args.entity.design + " class)";
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
          <p> Acceleration (G): {vectorToString(ship_next_accel)}</p>
        ) : (
          <></>
        )}
      </div>
    </div>
  );
}

export default Controls;
