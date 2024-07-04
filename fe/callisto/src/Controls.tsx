import { useContext, useState } from "react";
import {
  Entity,
  EntitiesServerContext,
  EntityRefreshCallback,
  FlightPlan
} from "./Contexts";

import { launchMissile } from "./ServerManager";

const POS_SCALE = 1000.0;

function AccelerationManager(args: {
  ships: Entity[];
  setAcceleration: (target: string, x: number, y: number, z: number) => void;
  setComputerShip: (entity: Entity) => void;
}) {
  function handleSubmit(
    ship: Entity
  ): (event: React.FormEvent<HTMLFormElement>) => void {
    return (event: React.FormEvent<HTMLFormElement>) => {
      console.log("Setting acceleration for " + ship.name);
      event.preventDefault();
      let x = Number(event.currentTarget.x.value);
      let y = Number(event.currentTarget.y.value);
      let z = Number(event.currentTarget.z.value);
      args.setAcceleration(ship.name, x, y, z);
    };
  }
  return (
    <>
      <h2 className="control-form">Set Accel</h2>
      {args.ships.map((ships) => (
        <form
          key={ships.name + "-accel-setter"}
          className="as-form"
          onSubmit={handleSubmit(ships)}>
          <label className="as-label" onDoubleClick={() => args.setComputerShip(ships)}>{ships.name}</label>
          <div>
            <input className="as-input" name="x" type="text" defaultValue={0} />
            <input className="as-input" name="y" type="text" defaultValue={0} />
            <input className="as-input" name="z" type="text" defaultValue={0} />
            <input className="as-input blue-button" type="submit" value="Set" />
          </div>
        </form>
      ))}
    </>
  );
}

function ShipComputer(args: {
  ship: Entity;
  setComputerShip: (ship: Entity | null) => void;
  currentPlan: FlightPlan | null;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number]
  ) => void;
}) {
  const [navigationTarget, setNavigationTarget] = useState({
    p_x: "0",
    p_y: "0",
    p_z: "0",
    v_x: "0",
    v_y: "0",
    v_z: "0",
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
    console.log(
      "Computing route for " + args.ship.name + " to " + end_pos + " " + end_vel
    );
    args.getAndShowPlan(args.ship.name, end_pos, end_vel);
  }

  function handleLaunchSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const formElements = form.elements as typeof form.elements & {
      missile_target: HTMLInputElement
    }
    console.log("Launching missile for " + args.ship.name + " to " + formElements.missile_target.value);
    launchMissile(args.ship.name, formElements.missile_target.value, serverEntities.handler)
  }

  let title = "Computer " + args.ship.name;

  return (
    <div>
      <h2>{title}</h2>
      <form className="control-form" onSubmit={handleNavigationSubmit}>
        <label className="control-label">Target Position</label>
        <div>
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
        <label className="control-label">Target Velocity</label>
        <div>
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
        <input
          className="control-input control-button blue-button"
          type="submit"
          value="Compute"
        />
      </form>
      <form className="control-form" onSubmit={handleLaunchSubmit}>
        <label className="control-label">Launch Missile
        <div className="control-launch-div">
          { /* <input
            className="control-name-input"
            name="missile_target"
            type="text"
            value={missileTarget}
            onChange={(event) => setMissileTarget(event.target.value)}
          /> */}
          <select
            className="control-name-input control-input"
            name="missile_target"
            id="missile_target">
            { serverEntities.entities.ships.filter((ship) => ship.name !== args.ship.name).map((ship) => (
              <option key={ship.name} value={ship.name}>{ship.name}</option>
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
      {args.currentPlan && (
        <div>
          <h2>Current Plan</h2>
          {args.currentPlan.accelerations.map(([accel, time], index) => (
            <div key={"accel-" + index}>
              <p>
                ({accel[0].toFixed(1)}, {accel[1].toFixed(1)},{" "}
                {accel[2].toFixed(1)}) for {time.toFixed(0)}s
              </p>
            </div>
          ))}
        </div>)}
      <button
        className="control-input control-button blue-button"
        onClick={() => {
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0]);
          args.setComputerShip(null);
        }}>
        Close
      </button>
    </div>
  );
}
function AddShip(args: { submitHandler: (ship: Entity) => void }) {
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
    let newShip: Entity = {
      name: addShip.name,
      position: [
        Number(addShip.xpos) * POS_SCALE,
        Number(addShip.ypos) * POS_SCALE,
        Number(addShip.zpos) * POS_SCALE,
      ],
      velocity: [
        Number(addShip.xvel),
        Number(addShip.yvel),
        Number(addShip.zvel),
      ],
      acceleration: [
        Number(addShip.xacc),
        Number(addShip.yacc),
        Number(addShip.zacc),
      ],
      kind: {
        Ship: {},
      },
    };
    console.log("Adding ship: " + JSON.stringify(newShip));

    args.submitHandler(newShip);
    addShipUpdate(initialShip);
  }

  return (
    <form className="control-form" onSubmit={handleSubmit}>
      <h2>Add Ship</h2>
      <label className="control-label">Name</label>
      <input
        className="control-name-input control-input"
        name="name"
        type="text"
        onChange={handleChange}
        value={addShip.name}
      />
      <label className="control-label">Position</label>
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
      <label className="control-label">Velocity</label>
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
      <label className="control-label">Acceleration</label>
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
      <input
        className="control-input control-button blue-button"
        type="submit"
        value="Create Ship"
      />
    </form>
  );
}

function Controls(args: {
  nextRound: (callback: EntityRefreshCallback) => void;
  addEntity: (entity: Entity, callback: EntityRefreshCallback) => void;
  setAcceleration: (
    target: string,
    acceleration: [number, number, number],
    callBack: EntityRefreshCallback
  ) => void;
  computerShip: Entity | null;
  setComputerShip: (ship: Entity | null) => void;
  currentPlan: FlightPlan | null;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number]
  ) => void;
}) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <div className="controls-pane">
      <h1>Controls</h1>
      <AddShip
        submitHandler={(entity) => args.addEntity(entity, serverEntities.handler)}
      />
      <AccelerationManager
        ships={serverEntities.entities.ships}
        setAcceleration={(target, x, y, z) => {
          args.setAcceleration(target, [x, y, z], serverEntities.handler);
        }}
        setComputerShip={args.setComputerShip}
      />
      {args.computerShip && (
        <ShipComputer
          ship={args.computerShip}
          setComputerShip={args.setComputerShip}
          currentPlan={args.currentPlan}
          getAndShowPlan={args.getAndShowPlan}
        />
      )}
      <button
        className="control-input control-button blue-button button-next-round"
        // Reset the computer and route on the next round.  If this gets any more complex move it into its
        // own function.
        onClick={() => {
          args.setComputerShip(null);
          args.getAndShowPlan(null, [0, 0, 0], [0, 0, 0]);
          args.nextRound(serverEntities.handler);
        }}>
        Next Round
      </button>
    </div>
  );
}

export default Controls;
