import { useContext, useState, useEffect, useRef } from "react";
import { Tooltip } from 'react-tooltip'
import * as THREE from "three";
import {
  EntitiesServerContext,
  EntityRefreshCallback,
  FlightPathResult,
  Ship,
  DEFAULT_ACCEL_DURATION,
  Acceleration,
  SCALE,
  ViewControlParams,
  Entity,
  Planet,
  USP_BEAM,
  USP_PULSE,
  USP_MISSILE,
} from "./Universal";

import { addShip, setPlan } from "./ServerManager";
import { validateUSP } from "./Ships";
import { scaleVector, vectorToString } from "./Util";

import { CiCircleQuestion } from "react-icons/ci";

import { ReactComponent as BeamIcon } from "./icons/laser.svg";
import { ReactComponent as PulseIcon } from "./icons/laser.svg";
import { ReactComponent as Missile } from "./icons/missile.svg";

const POS_SCALE = 1000.0;

const FIRE_ACTION_NAME = ["Beam", "Pulse", "Missile"];

const FIRE_ACTION_BEAM = FIRE_ACTION_NAME[0];
const FIRE_ACTION_PULSE = FIRE_ACTION_NAME[1];
const FIRE_ACTION_MISSILE = FIRE_ACTION_NAME[2];

class FireAction {
  kind: string;
  target: string;
  constructor(kind: string, target: string) {
    this.kind = kind;
    this.target = target;
  }
}

export type FireState = FireAction[];

export function stringifyFireState(actions: Map<String, FireState>) {
  return JSON.stringify(Array.from(actions.entries()));
}

function ShipList(args: {
  computerShipName: string | null;
  setComputerShipName: (shipName: string | null) => void;
  setCameraPos: (pos: THREE.Vector3) => void;
  camera: THREE.Camera | null;
}) {
  const serverEntities = useContext(EntitiesServerContext);

  const ships = serverEntities.entities.ships;

  const selectRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (selectRef.current != null) {
      selectRef.current.value =
        (args.computerShipName && args.computerShipName) || "";
    }
  }, [args.computerShipName]);

  function handleShipListSelectChange(
    event: React.ChangeEvent<HTMLSelectElement>
  ) {
    let value = event.target.value;

    let selectedShip = serverEntities.entities.ships.find(
      (ship) => ship.name === value
    );

    if (selectedShip == null) {
      args.setComputerShipName(null);
    } else {
      args.setComputerShipName(selectedShip.name);
    }
  }

  function moveCameraToShip() {
    if (args.camera == null) {
      console.log("Cannot move camera because camera object in Three is null.");
      return;
    }
    if (args.computerShipName) {
      let ship = serverEntities.entities.ships.find(
        (ship) => ship.name === args.computerShipName
      );
      if (ship) {
        const downCamera = new THREE.Vector3(0, 0, 40);
        downCamera.applyQuaternion(args.camera.quaternion);
        let new_camera_pos = new THREE.Vector3(
          ship.position[0] * SCALE,
          ship.position[1] * SCALE,
          ship.position[2] * SCALE
        ).add(downCamera);
        args.setCameraPos(new_camera_pos);
      }
    }
  }

  return (
    <div className="control-launch-div">
      <h2 className="ship-list-label">Ship: </h2>
      <select
        className="select-dropdown control-name-input control-input"
        name="ship_list_choice"
        ref={selectRef}
        defaultValue={args.computerShipName || ""}
        onChange={handleShipListSelectChange}>
        <option key="none" value=""></option>
        {ships.map((ship) => (
          <option key={ship.name + "-ship_list"}>{ship.name}</option>
        ))}
      </select>
      <button className="control-input blue-button" onClick={moveCameraToShip}>
        Go
      </button>
    </div>
  );
}

export function NavigationPlan(args: {
  plan: [Acceleration, Acceleration | null];
}) {
  function prettyPrintAccel(accel: Acceleration) {
    let ax = accel[0][0].toFixed(2).padStart(5, " ");
    let ay = accel[0][1].toFixed(2).padStart(6, " ");
    let az = accel[0][2].toFixed(2).padStart(6, " ");
    let time = accel[1].toFixed(0).padStart(4, " ");
    let s = `${time}s @ (${ax},${ay},${az})`;
    return s;
  }

  let accel0 = args.plan[0];
  let accel1 = args.plan[1];

  return (
    <>
      <div key={"accel-0"}>
        <pre className="plan-accel-text">{prettyPrintAccel(accel0)}</pre>
      </div>
      {accel1 && (
        <div key={"accel-1"}>
          <pre className="plan-accel-text">{prettyPrintAccel(accel1)}</pre>
        </div>
      )}
    </>
  );
}

export function ShipComputer(args: {
  shipName: string;
  setComputerShipName: (shipName: string | null) => void;
  proposedPlan: FlightPathResult | null;
  resetProposedPlan: () => void;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null,
    standoff: number
  ) => void;
}) {
  const serverEntities = useContext(EntitiesServerContext);

  // A bit of a hack to make ship defined.  If we get here and it cannot find the ship in the entities table something is very very wrong.
  const ship =
    serverEntities.entities.ships.find((ship) => ship.name === args.shipName) ||
    new Ship(
      "Error",
      [0, 0, 0],
      [0, 0, 0],
      [[[0, 0, 0], 0], null],
      "0000000-00000-0"
    );

  if (ship == null) {
    console.error(
      `(ShipComputer) Unable to find ship of name "${args.shipName}!`
    );
  }

  const selectRef = useRef<HTMLSelectElement>(null);

  const initNavigationTargetState = {
    p_x: "0",
    p_y: "0",
    p_z: "0",
    v_x: "0",
    v_y: "0",
    v_z: "0",
    standoff: "0",
  };

  const [navigationTarget, setNavigationTarget] = useState(
    initNavigationTargetState
  );

  let startAccel = [
    ship?.plan[0][0][0].toString(),
    ship?.plan[0][0][1].toString(),
    ship?.plan[0][0][2].toString(),
  ];

  const [computerAccel, setComputerAccel] = useState({
    x: startAccel[0],
    y: startAccel[1],
    z: startAccel[2],
  });

  function handleNavigationChange(event: React.ChangeEvent<HTMLInputElement>) {
    setNavigationTarget({
      ...navigationTarget,
      [event.target.name]: event.target.value,
    });
  }

  function handleNavigationSubmit(event: React.FormEvent<HTMLFormElement>) {
    // Perform computation logic here
    event.preventDefault();

    let end_pos: [number, number, number] = [
      Number(navigationTarget.p_x) * POS_SCALE,
      Number(navigationTarget.p_y) * POS_SCALE,
      Number(navigationTarget.p_z) * POS_SCALE,
    ];
    let end_vel: [number, number, number] = [
      Number(navigationTarget.v_x),
      Number(navigationTarget.v_y),
      Number(navigationTarget.v_z),
    ];
    let target_vel: [number, number, number] | null = [
      Number(navigationTarget.v_x),
      Number(navigationTarget.v_y),
      Number(navigationTarget.v_z),
    ];

    let standoff = Number(navigationTarget.standoff) * POS_SCALE;

    console.log(
      `Computing route for ${ship.name} to ${end_pos} ${end_vel} with target velocity ${target_vel} with standoff ${standoff}`
    );
    args.getAndShowPlan(ship.name, end_pos, end_vel, target_vel, standoff);
  }

  function handleNavTargetSelectChange(
    event: React.ChangeEvent<HTMLSelectElement>
  ) {
    let value = event.target.value;
    let shipTarget = serverEntities.entities.ships.find(
      (ship) => ship.name === value
    );
    let planetTarget = serverEntities.entities.planets.find(
      (planet) => planet.name === value
    );

    if (shipTarget == null && planetTarget == null) {
      console.error(
        `(Controls.handleNavTargetSelectChange) Cannot find navigation target {${value}}`
      );
    }

    let p_x = 0;
    let p_y = 0;
    let p_z = 0;
    let v_x = 0;
    let v_y = 0;
    let v_z = 0;
    let standoff = 0;

    if (shipTarget != null) {
      p_x = shipTarget.position[0] / POS_SCALE;
      p_y = shipTarget.position[1] / POS_SCALE;
      p_z = shipTarget.position[2] / POS_SCALE;
      v_x = shipTarget.velocity[0];
      v_y = shipTarget.velocity[1];
      v_z = shipTarget.velocity[2];
      standoff = 1000;
    } else if (planetTarget != null) {
      p_x = planetTarget.position[0] / POS_SCALE;
      p_y = planetTarget.position[1] / POS_SCALE;
      p_z = planetTarget.position[2] / POS_SCALE;
      v_x = planetTarget.velocity[0];
      v_y = planetTarget.velocity[1];
      v_z = planetTarget.velocity[2];
      standoff = (planetTarget.radius * 1.1) / POS_SCALE;
    }

    setNavigationTarget({
      p_x: p_x.toFixed(0),
      p_y: p_y.toFixed(0),
      p_z: p_z.toFixed(0),
      v_x: v_x.toFixed(1),
      v_y: v_y.toFixed(1),
      v_z: v_z.toFixed(1),
      standoff: standoff.toFixed(1),
    });
  }

  function handleAssignPlan() {
    if (args.proposedPlan == null) {
      console.error(`(Controls.handleAssignPlan) No current plan`);
    } else {
      setComputerAccel({
        x: args.proposedPlan.plan[0][0][0].toString(),
        y: args.proposedPlan.plan[0][0][1].toString(),
        z: args.proposedPlan.plan[0][0][2].toString(),
      });
      setPlan(ship.name, args.proposedPlan.plan, serverEntities.handler)
      .then(() => args.resetProposedPlan());

      if (selectRef.current !== null) {
        selectRef.current.value = "";
      }

      setNavigationTarget(initNavigationTargetState);
    }
  }

  // Intentionally defining as a function that returns JSX vs a true component.  If I use a true component then
  // we lose focus on each key stroke.  But I do need accelerationManager nested inside ShipComputer as we want to share
  // the computerAccel state between this component and the navigation computer functionality.
  function accelerationManager() {
    function handleSetAcceleration(event: React.FormEvent<HTMLFormElement>) {
      event.preventDefault();
      let x = Number(computerAccel.x);
      let y = Number(computerAccel.y);
      let z = Number(computerAccel.z);

      let setColor = (id: string, color: string) => {
        let elem = document.getElementById(id);
        if (elem !== null) {
          elem.style.color = color;
        }
      }

      setPlan(
        ship.name,
        [[[x, y, z], DEFAULT_ACCEL_DURATION], null],
        serverEntities.handler
      )
        .then(() => {
          setColor("control-input-x", "black");
          setColor("control-input-y", "black");
          setColor("control-input-z", "black");

          args.resetProposedPlan();
        })
        .catch((error) => {
          setColor("control-input-x", "red");
          setColor("control-input-y", "red");
          setColor("control-input-z", "red");

          args.resetProposedPlan();
        });
    }

    function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
      setComputerAccel({
        ...computerAccel,
        [event.target.name]: event.target.value,
      });
    }

    return (
      <>
        {" "}
        <h2 className="control-form">
          Set Accel (m/s<sup>2</sup>)
        </h2>
        <form
          key={ship.name + "-accel-setter"}
          className="as-form"
          onSubmit={handleSetAcceleration}>
          <input
            className="control-input"
            id ="control-input-x"
            name="x"
            type="text"
            onChange={handleChange}
            value={computerAccel.x}
          />
          <input
            className="control-input"
            id ="control-input-y"
            name="y"
            type="text"
            onChange={handleChange}
            value={computerAccel.y}
          />
          <input
            className="control-input"
            id ="control-input-z"
            name="z"
            type="text"
            onChange={handleChange}
            value={computerAccel.z}
          />
          <input
            className="control-input control-button blue-button"
            type="submit"
            value="Set"
          />
        </form>
      </>
    );
  }

  let title = ship.name + " Nav";

  return (
    <div id="computer-window" className="computer-window">
      <h1>{title}</h1>
      {accelerationManager()}
      <hr />
      <button
        className="control-input control-button blue-button"
        onClick={() => {
          setNavigationTarget({
            p_x: (ship.position[0] / POS_SCALE).toString(),
            p_y: (ship.position[1] / POS_SCALE).toString(),
            p_z: (ship.position[2] / POS_SCALE).toString(),
            v_x: "0",
            v_y: "0",
            v_z: "0",
            standoff: "0",
          });
          args.getAndShowPlan(ship.name, ship.position, [0, 0, 0], null, 0);
        }}>
        Full Stop
      </button>
      <hr />
      <h2 className="control-form">Navigation</h2>
      <form className="target-entry-form" onSubmit={handleNavigationSubmit}>
        <label className="control-label" style={{ display: "flex" }}>
          Nav Target:
          <select
            className="select-dropdown control-name-input control-input"
            ref={selectRef}
            name="navigation_target"
            onChange={handleNavTargetSelectChange}>
            <option key="none" value=""></option>
            {serverEntities.entities.ships
              .filter((candidate) => candidate.name !== ship.name)
              .map((notMeShip) => (
                <option key={notMeShip.name} value={notMeShip.name}>
                  {notMeShip.name}
                </option>
              ))}
            {serverEntities.entities.planets.map((planet) => (
              <option key={planet.name} value={planet.name}>
                {planet.name}
              </option>
            ))}
          </select>
        </label>
        <div className="target-details-div">
          <label className="control-label">
            Target Position (km)
            <div style={{ display: "flex" }} className="coordinate-input">
              <input
                className="control-input"
                name="p_x"
                type="text"
                value={navigationTarget.p_x}
                onChange={handleNavigationChange}
              />
              <input
                className="control-input"
                name="p_y"
                type="text"
                value={navigationTarget.p_y}
                onChange={handleNavigationChange}
              />
              <input
                className="control-input"
                name="p_z"
                type="text"
                value={navigationTarget.p_z}
                onChange={handleNavigationChange}
              />
            </div>
          </label>
          <label className="control-label">
            Target Velocity (m/s)
            <div style={{ display: "flex" }} className="coordinate-input">
              <input
                className="control-input"
                name="v_x"
                type="text"
                value={navigationTarget.v_x}
                onChange={handleNavigationChange}
              />
              <input
                className="control-input"
                name="v_y"
                type="text"
                value={navigationTarget.v_y}
                onChange={handleNavigationChange}
              />
              <input
                className="control-input"
                name="v_z"
                type="text"
                value={navigationTarget.v_z}
                onChange={handleNavigationChange}
              />
            </div>
          </label>
          <label
            className="control-label"
            style={{ display: "flex", justifyContent: "space-between" }}>
            Standoff (km)
            <div className="coordinate-input">
              <input
                className="control-input standoff-input"
                name="standoff"
                type="text"
                value={navigationTarget.standoff}
                onChange={handleNavigationChange}
              />
            </div>
          </label>
        </div>
        <input
          className="control-input control-button blue-button"
          type="submit"
          value="Compute"
        />
      </form>
      {args.proposedPlan && (
        <div>
          <h2 className="control-form">Proposed Plan</h2>
          <NavigationPlan plan={args.proposedPlan.plan} />
          <button
            className="control-input control-button blue-button"
            onClick={handleAssignPlan}>
            Assign Plan
          </button>
        </div>
      )}
      <button
        className="control-input control-button blue-button"
        onClick={() => {
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, 0);
          args.setComputerShipName(null);
        }}>
        Close
      </button>
    </div>
  );
}
function AddShip(args: {
  submitHandler: (
    name: string,
    position: [number, number, number],
    velocity: [number, number, number],
    acceleration: [number, number, number],
    usp: string
  ) => void;
}) {
  let uspRef = useRef<HTMLInputElement>(null);

  const initialShip = {
    name: "ShipName",
    xpos: "0",
    ypos: "0",
    zpos: "0",
    xvel: "0",
    yvel: "0",
    zvel: "0",
    usp: "0000000-00000-0",
  };

  const [addShip, addShipUpdate] = useState(initialShip);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    if (uspRef.current) {
      uspRef.current.style.color = "black";
    }

    addShipUpdate({ ...addShip, [event.target.name]: event.target.value });
  }

  function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    let name = addShip.name;
    let position: [number, number, number] = [
      Number(addShip.xpos) * POS_SCALE,
      Number(addShip.ypos) * POS_SCALE,
      Number(addShip.zpos) * POS_SCALE,
    ];
    let velocity: [number, number, number] = [
      Number(addShip.xvel),
      Number(addShip.yvel),
      Number(addShip.zvel),
    ];

    let usp: string = addShip.usp.replace(/-/g, "");
    console.log(usp);
    usp =
      usp.substring(0, 7) +
      "-" +
      usp.substring(7, 12) +
      "-" +
      usp.substring(12);
    addShipUpdate({ ...addShip, usp: usp });

    console.log(
      `Adding Ship ${name}: Position ${position}, Velocity ${velocity}, USP ${usp}`
    );

    let [valid, error] = validateUSP(usp);

    if (!valid) {
      uspRef.current?.focus();
      console.log("Invalid USP: " + error);
      if (uspRef.current) {
        uspRef.current.style.color = "red";
      }
      return;
    }

    args.submitHandler(name, position, velocity, [0, 0, 0], usp);
    addShipUpdate(initialShip);
  }

  return (
    <form className="control-form" onSubmit={handleSubmit}>
      <h2>Add Ship</h2>
      <label className="control-label">
        Name
        <input
          className="control-name-input control-input"
          name="name"
          type="text"
          onChange={handleChange}
          value={addShip.name}
        />
      </label>
      <label className="control-label">
        Position (km)
        <div className="coordinate-input">
          <input
            className="control-input"
            name="xpos"
            type="text"
            value={addShip.xpos}
            onChange={handleChange}
          />
          <input
            className="control-input"
            name="ypos"
            type="text"
            value={addShip.ypos}
            onChange={handleChange}
          />
          <input
            className="control-input"
            name="zpos"
            type="text"
            value={addShip.zpos}
            onChange={handleChange}
          />
        </div>
      </label>
      <label className="control-label">
        Velocity (m/s)
        <div className="coordinate-input">
          <input
            className="control-input"
            name="xvel"
            type="text"
            value={addShip.xvel}
            onChange={handleChange}
          />
          <input
            className="control-input"
            name="yvel"
            type="text"
            value={addShip.yvel}
            onChange={handleChange}
          />
          <input
            className="control-input"
            name="zvel"
            type="text"
            value={addShip.zvel}
            onChange={handleChange}
          />
        </div>
      </label>
      <label className="control-label">
        <div> USP
        <CiCircleQuestion data-tooltip-id="usp-help-tooltip" data-tooltip-variant="info" data-tooltip-place="bottom" data-tooltip-position-strategy="absolute"/>
        <Tooltip id="usp-help-tooltip" className="info-tooltip"
        render = {() => (
        <span className="tooltip-content">
          <h3>USP Description</h3>
        <p>The USP is a 13 characters in hex:</p>
        <ul>
          <li>hull, armor, jump, maneuver, </li>
          <li>powerplant, computer, crew</li>
          <li>beam, pulse, particle, </li>
          <li>missile, sand</li>
          <li>tech level</li>
        </ul>
        </span >)}/>
        </div>
        <input
          ref={uspRef}
          className="control-input usp-input"
          name="usp"
          type="text"
          value={addShip.usp}
          onChange={handleChange}
        />
      </label>
      <input
        className="control-input control-button blue-button"
        type="submit"
        value="Create Ship"
      />
    </form>
  );
}

function FireActions(args: { actions: FireState }) {
  return (
    <div className="control-form">
      <h2>Fire Actions</h2>
      {args.actions.map((action, index) =>
        action.kind === FIRE_ACTION_BEAM ? (
          <p key={index + "_fire_img"}>
            <BeamIcon className="beam-type-icon" /> to {action.target}
          </p>
        ) : action.kind === FIRE_ACTION_PULSE ? (
          <p key={index + "_fire_img"}>
            <PulseIcon className="pulse-type-icon" /> to {action.target}
          </p>
        ) : (
          <p key={index + "_fire_img"}>
            <Missile className="missile-type-icon" /> to {action.target}
          </p>
        )
      )}
    </div>
  );
}

export function Controls(args: {
  nextRound: (
    fireActions: Map<string, FireState>,
    callback: EntityRefreshCallback
  ) => void;
  computerShipName: string | null;
  setComputerShipName: (shipName: string | null) => void;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null,
    standoff: number
  ) => void;
  setCameraPos: (pos: THREE.Vector3) => void;
  camera: THREE.Camera | null;
}) {
  const [fire_actions, setFireActions] = useState(new Map<string, FireState>());
  const [action, setAction] = useState(FIRE_ACTION_NAME[0]);

  const serverEntities = useContext(EntitiesServerContext);

  const computerShip = serverEntities.entities.ships.find(
    (ship) => ship.name === args.computerShipName
  );

  function handleFireSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const formElements = form.elements as typeof form.elements & {
      fire_target: HTMLInputElement;
    };

    if (computerShip) {
      console.log(
        "Fire " +
          action +
          " for " +
          computerShip.name +
          " to " +
          formElements.fire_target.value
      );

      let new_actions = new Map(fire_actions);
      let current_ship_actions = new_actions.get(computerShip.name);
      let new_action = new FireAction(action, formElements.fire_target.value);
      current_ship_actions = [...(current_ship_actions || []), new_action];
      new_actions.set(computerShip.name, current_ship_actions);

      setFireActions(new_actions);
    }
  }

  return (
    <div className="controls-pane">
      <h1>Controls</h1>
      <AddShip
        submitHandler={(
          name: string,
          position: [number, number, number],
          velocity: [number, number, number],
          acceleration: [number, number, number],
          usp: string
        ) =>
          addShip(
            name,
            position,
            velocity,
            acceleration,
            usp,
            serverEntities.handler
          )
        }
      />
      <hr />
      <ShipList
        computerShipName={args.computerShipName}
        setComputerShipName={args.setComputerShipName}
        setCameraPos={args.setCameraPos}
        camera={args.camera}
      />
      {computerShip && (
        <>
          <h2 className="control-form">USP</h2>
          <pre className="plan-accel-text">{computerShip.usp}</pre>
          <h2 className="control-form">Current Position</h2>
          <pre className="plan-accel-text">
            {"(" +
              (computerShip.position[0] / POS_SCALE).toFixed(0) +
              ", " +
              (computerShip.position[1] / POS_SCALE).toFixed(0) +
              ", " +
              (computerShip.position[2] / POS_SCALE).toFixed(0) +
              ")"}
          </pre>
          <h2 className="control-form">
            Current Plan (s @ m/s<sup>2</sup>)
          </h2>
          <NavigationPlan plan={computerShip.plan} />
          <hr />
          <form className="control-form" onSubmit={handleFireSubmit}>
            <label className="control-label">
              <h2>Fire Control</h2>
              <div className="control-launch-div">
                <select
                  className="control-name-input control-input"
                  name="fire_target"
                  id="fire_target">
                  {serverEntities.entities.ships
                    .filter((candidate) => candidate.name !== computerShip.name)
                    .map((notMeShip) => (
                      <option key={notMeShip.name} value={notMeShip.name}>
                        {notMeShip.name}
                      </option>
                    ))}
                </select>
                {computerShip.usp.substring(USP_MISSILE, USP_MISSILE + 1) !==
                  "0" && (
                  <input
                    onClick={(e) => setAction(FIRE_ACTION_MISSILE)}
                    className="control-launch-button blue-button"
                    type="submit"
                    value="Missile"
                  />
                )}
                {computerShip.usp.substring(USP_BEAM, USP_BEAM + 1) !== "0" && (
                  <input
                    className="control-launch-button blue-button"
                    onClick={(e) => setAction(FIRE_ACTION_BEAM)}
                    type="submit"
                    value="Beam"
                  />
                )}
                {computerShip.usp.substring(USP_PULSE, USP_PULSE + 1) !==
                  "0" && (
                  <input
                    className="control-launch-button blue-button"
                    onClick={(e) => setAction(FIRE_ACTION_PULSE)}
                    type="submit"
                    value="Pulse"
                  />
                )}
              </div>
            </label>
          </form>
        </>
      )}
      {computerShip &&
        (fire_actions.get(computerShip.name) || []).length > 0 && (
          <FireActions actions={fire_actions.get(computerShip?.name) || []} />
        )}
      <button
        className="control-input control-button blue-button button-next-round"
        // Reset the computer and route on the next round.  If this gets any more complex move it into its
        // own function.
        onClick={() => {
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, 0);
          args.nextRound(fire_actions, serverEntities.handler);
          setFireActions(new Map<string, FireState>());
          args.setComputerShipName(null);
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
              gravityWells: !args.viewControls.gravityWells,
            })
          }
        />{" "}
        Gravity Well
      </label>
    </div>
  );
}

export function EntityInfoWindow(args: { entity: Entity }) {
  let isPlanet = false;
  let isShip = false;
  let ship_next_accel: [number, number, number] = [0, 0, 0];
  let radiusKm = 0;

  if (args.entity instanceof Planet) {
    isPlanet = true;
    radiusKm = args.entity.radius / 1000.0;
  } else if (args.entity instanceof Ship) {
    isShip = true;
    ship_next_accel = args.entity.plan[0][0];
  }

  return (
    <div id="ship-info-window" className="ship-info-window">
      <h2 className="ship-info-title">{args.entity.name}</h2>
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
