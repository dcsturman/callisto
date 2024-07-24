import { useContext, useState } from "react";
import {
  EntitiesServerContext,
  EntityRefreshCallback,
  FlightPathResult,
  Ship,
  DEFAULT_ACCEL_DURATION,
} from "./Universal";

import { addShip, setPlan } from "./ServerManager";

import { launchMissile } from "./ServerManager";

const POS_SCALE = 1000.0;

function ShipList(args: { setComputerShip: (entity: Ship) => void }) {
  const serverEntities = useContext(EntitiesServerContext);
  const ships = serverEntities.entities.ships;

  return (
    <>
      <h2 className="control-form">Ship List</h2>
      {ships.map((ship) => (
        <div
          key={ship.name + "-accel-setter"}
          className="as-label clickable-label"
          onDoubleClick={() => args.setComputerShip(ship)}>
          {ship.name}
        </div>
      ))}
    </>
  );
}

export function ShipComputer(args: {
  ship: Ship;
  setComputerShip: (ship: Ship | null) => void;
  currentPlan: FlightPathResult | null;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null,
    standoff: number
  ) => void;
}) {
  const [navigationTarget, setNavigationTarget] = useState({
    p_x: "0",
    p_y: "0",
    p_z: "0",
    v_x: "0",
    v_y: "0",
    v_z: "0",
    standoff: "0",
  });

  let startAccel = [
    args.ship.plan[0][0][0].toString(),
    args.ship.plan[0][0][1].toString(),
    args.ship.plan[0][0][2].toString(),
  ];

  const [computerAccel, setComputerAccel] = useState({
    x: startAccel[0],
    y: startAccel[1],
    z: startAccel[2],
  });

  const serverEntities = useContext(EntitiesServerContext);

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
    
    let standoff =  Number(navigationTarget.standoff) * POS_SCALE;
      
    console.log(
      `Computing route for ${args.ship.name} to ${end_pos} ${end_vel} with target velocity ${target_vel} with standoff ${standoff}`
    );
    args.getAndShowPlan(args.ship.name, end_pos, end_vel, target_vel, standoff);
  }

  function handleLaunchSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const formElements = form.elements as typeof form.elements & {
      missile_target: HTMLInputElement;
    };
    console.log(
      "Launching missile for " +
        args.ship.name +
        " to " +
        formElements.missile_target.value
    );
    launchMissile(
      args.ship.name,
      formElements.missile_target.value,
      serverEntities.handler
    );
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
      standoff = planetTarget.radius * 1.1 / POS_SCALE;
    }

    setNavigationTarget({
      p_x: p_x.toString(),
      p_y: p_y.toString(),
      p_z: p_z.toString(),
      v_x: v_x.toString(),
      v_y: v_y.toString(),
      v_z: v_z.toString(),
      standoff: standoff.toString(),
    });
  }

  function handleAssignPlan() {
    let ship = serverEntities.entities.ships.find(
      (ship) => ship.name === args.ship.name
    );

    if (ship == null) {
      console.error(
        `(Controls.handleAssignPlan) Cannot find ship {${args.ship.name}}`
      );
    }
    if (args.currentPlan == null) {
      console.error(`(Controls.handleAssignPlan) No current plan`);
    } else {
      setComputerAccel({
        x: args.currentPlan.plan[0][0][0].toString(),
        y: args.currentPlan.plan[0][0][1].toString(),
        z: args.currentPlan.plan[0][0][2].toString(),
      });
      setPlan(args.ship.name, args.currentPlan.plan, serverEntities.handler);
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
        args.ship.name,
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
          key={args.ship.name + "-accel-setter"}
          className="as-form"
          onSubmit={handleSetAcceleration}>
          <div>
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
              className="control-input blue-button"
              type="submit"
              value="Set"
            />
          </div>
        </form>{" "}
      </>
    );
  }

  let title = "Computer " + args.ship.name;
  let accel0 = args.currentPlan?.plan[0];
  let accel1 = args.currentPlan?.plan[1];

  return (
    <div id="computer-window" className="computer-window">
      <h1>{title}</h1>
      {accelerationManager()}
      <hr />
      <h2 className="control-form">Navigation Computer</h2>
      <form className="target-entry-form" onSubmit={handleNavigationSubmit} >
        <label className="control-label" style={{ display: "flex" }}>
          Nav Target:
          <select
            className="navigation-target-select control-name-input control-input"
            name="navigation_target"
            onChange={handleNavTargetSelectChange}>
            <option key="none" value=""></option>
            {serverEntities.entities.ships
              .filter((ship) => ship.name !== args.ship.name)
              .map((ship) => (
                <option key={ship.name} value={ship.name}>
                  {ship.name}
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
          <div className="target-specifics-div">
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
          </div>
          <label className="control-label" style={{ display: "flex", justifyContent: "space-between"}}>
            Standoff:
            <div  className="coordinate-input">
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
      {accel0 && (
        <div>
          <h2 className="control-form">Current Plan</h2>
          <div key={"accel-0"}>
            <p>
              ({accel0[0][0].toFixed(1)}, {accel0[0][1].toFixed(1)},{" "}
              {accel0[0][2].toFixed(1)}) for {accel0[1].toFixed(0)}s
            </p>
          </div>
          {accel1 && (
            <div key={"accel-1"}>
              <p>
                ({accel1[0][0].toFixed(1)}, {accel1[0][1].toFixed(1)},{" "}
                {accel1[0][2].toFixed(1)}) for {accel1[1].toFixed(0)}s
              </p>
            </div>
          )}
          ))
          <button
            className="control-input control-button blue-button"
            onClick={handleAssignPlan}>
            Assign Plan
          </button>
        </div>
      )}
      <hr />
      <form className="control-form" onSubmit={handleLaunchSubmit}>
        <label className="control-label">
          <h2>Launch Missile</h2>
          <div className="control-launch-div">
            <select
              className="control-name-input control-input"
              name="missile_target"
              id="missile_target">
              {serverEntities.entities.ships
                .filter((ship) => ship.name !== args.ship.name)
                .map((ship) => (
                  <option key={ship.name} value={ship.name}>
                    {ship.name}
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
      <button
        className="control-input control-button blue-button"
        onClick={() => {
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, 0);
          args.setComputerShip(null);
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
    xacc: "0",
    yacc: "0",
    zacc: "0",
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
    let acceleration: [number, number, number] = [
      Number(addShip.xacc),
      Number(addShip.yacc),
      Number(addShip.zacc),
    ];

    console.log(
      `Adding Ship ${name}: Position ${position}, Velocity ${velocity}, Acceleration ${acceleration}`
    );

    args.submitHandler(name, position, velocity, acceleration);
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
      <label className="control-label">
        Acceleration
        <div className="coordinate-input">
          <input
            className="control-input"
            name="xacc"
            type="text"
            value={addShip.xacc}
            onChange={handleChange}
          />
          <input
            className="control-input"
            name="yacc"
            type="text"
            value={addShip.yacc}
            onChange={handleChange}
          />
          <input
            className="control-input"
            name="zacc"
            type="text"
            value={addShip.zacc}
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
  computerShip: Ship | null;
  setComputerShip: (ship: Ship | null) => void;
  currentPlan: FlightPathResult | null;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null,
    standoff: number
  ) => void;
}) {
  const serverEntities = useContext(EntitiesServerContext);

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
      <ShipList setComputerShip={args.setComputerShip} />
      <button
        className="control-input control-button blue-button button-next-round"
        // Reset the computer and route on the next round.  If this gets any more complex move it into its
        // own function.
        onClick={() => {
          args.setComputerShip(null);
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, 0);
          args.nextRound(serverEntities.handler);
        }}>
        Next Round
      </button>
    </div>
  );
}

export default Controls;
