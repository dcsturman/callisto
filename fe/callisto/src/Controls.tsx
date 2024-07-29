import { useContext, useState, useEffect, useRef } from "react";
import * as THREE from "three";
import {
  EntitiesServerContext,
  EntityRefreshCallback,
  FlightPathResult,
  Ship,
  DEFAULT_ACCEL_DURATION,
  Acceleration,
  SCALE
} from "./Universal";

import { addShip, setPlan, launchMissile } from "./ServerManager";

const POS_SCALE = 1000.0;

function ShipList(args: {
  computerShipName: string | null;
  setComputerShipName: (shipName: string | null) => void;
  setCameraPos: (pos: THREE.Vector3) => void;
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
    if (args.computerShipName) {
      let ship = serverEntities.entities.ships.find(
        (ship) => ship.name === args.computerShipName
      );
      if (ship) {
        args.setCameraPos(new THREE.Vector3(
          ship.position[0] * SCALE - 40,
          ship.position[1] * SCALE,
          ship.position[2] * SCALE
        ));
      }
    }
  }

  return (
    <div className="control-launch-div">
      <h2 className="ship-list-label">Ship: </h2>
      <select
        className="select-dropdown control-name-input control-input"
        name="shiplist_choice"
        ref={selectRef}
        defaultValue={args.computerShipName || ""}
        onChange={handleShipListSelectChange}>
        <option key="none" value=""></option>
        {ships.map((ship) => (
          <option key={ship.name + "-shiplist"}>{ship.name}</option>
        ))}
      </select>
      <button className="control-input blue-button" onClick={moveCameraToShip}>Go</button>
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
    new Ship("Error", [0, 0, 0], [0, 0, 0], [[[0, 0, 0], 0], null]);

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
      setPlan(ship.name, args.proposedPlan.plan, serverEntities.handler);
      args.resetProposedPlan();

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
      setPlan(
        ship.name,
        [[[x, y, z], DEFAULT_ACCEL_DURATION], null],
        serverEntities.handler
      );
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
        <h2 className="control-form">Set Accel</h2>
        <form
          key={ship.name + "-accel-setter"}
          className="as-form"
          onSubmit={handleSetAcceleration}>
          <input
            className="control-input"
            name="x"
            type="text"
            onChange={handleChange}
            value={computerAccel.x}
          />
          <input
            className="control-input"
            name="y"
            type="text"
            onChange={handleChange}
            value={computerAccel.y}
          />
          <input
            className="control-input"
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
            Target Position
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
            Target Velocity
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
            Standoff:
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
    acceleration: [number, number, number]
  ) => void;
}) {
  const initialShip = {
    name: "ShipName",
    xpos: "0",
    ypos: "0",
    zpos: "0",
    xvel: "0",
    yvel: "0",
    zvel: "0",
  };

  const [addShip, addShipUpdate] = useState(initialShip);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
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

    console.log(
      `Adding Ship ${name}: Position ${position}, Velocity ${velocity}`
    );

    args.submitHandler(name, position, velocity, [0, 0, 0]);
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
        Position
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
        Velocity
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
      <input
        className="control-input control-button blue-button"
        type="submit"
        value="Create Ship"
      />
    </form>
  );
}

export function Controls(args: {
  nextRound: (callback: EntityRefreshCallback) => void;
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
}) {
  const serverEntities = useContext(EntitiesServerContext);

  const computerShip = serverEntities.entities.ships.find(
    (ship) => ship.name === args.computerShipName
  );

  function handleLaunchSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const formElements = form.elements as typeof form.elements & {
      missile_target: HTMLInputElement;
    };
    if (computerShip) {
      console.log(
        "Launching missile for " +
          computerShip.name +
          " to " +
          formElements.missile_target.value
      );

      launchMissile(
        computerShip.name,
        formElements.missile_target.value,
        serverEntities.handler
      );
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
          acceleration: [number, number, number]
        ) =>
          addShip(
            name,
            position,
            velocity,
            acceleration,
            serverEntities.handler
          )
        }
      />
      <hr />
      <ShipList
        computerShipName={args.computerShipName}
        setComputerShipName={args.setComputerShipName}
        setCameraPos={args.setCameraPos}
      />
      {computerShip && (
        <>
          <h2 className="control-form">Current Position</h2>
          <pre className="plan-accel-text">
              {("(" + computerShip.position[0].toFixed(0) + ", " 
              + computerShip.position[1].toFixed(0) + ", "
              + computerShip.position[2].toFixed(0) + ")") }
          </pre>
          <h2 className="control-form">Current Plan</h2>
          <NavigationPlan plan={computerShip.plan} />
          <hr />
          <form className="control-form" onSubmit={handleLaunchSubmit}>
            <label className="control-label">
              <h2>Missile</h2>
              <div className="control-launch-div">
                <select
                  className="control-name-input control-input"
                  name="missile_target"
                  id="missile_target">
                  {serverEntities.entities.ships
                    .filter((candidate) => candidate.name !== computerShip.name)
                    .map((notMeShip) => (
                      <option key={notMeShip.name} value={notMeShip.name}>
                        {notMeShip.name}
                      </option>
                    ))}
                </select>
                <input
                  className="control-launch-button blue-button"
                  type="submit"
                  value="Launch"
                />
              </div>
            </label>
          </form>
        </>
      )}
      <button
        className="control-input control-button blue-button button-next-round"
        // Reset the computer and route on the next round.  If this gets any more complex move it into its
        // own function.
        onClick={() => {
          args.setComputerShipName(null);
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, 0);
          args.nextRound(serverEntities.handler);
        }}>
        Next Round
      </button>
    </div>
  );
}

export default Controls;
