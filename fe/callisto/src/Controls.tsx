import { useReducer, useContext, useState } from "react";
import {
  Entity,
  EntitiesServerContext,
  EntitiesServerUpdateContext,
  EntityRefreshCallback,
} from "./Contexts";

function AccelerationManager(args: {entities: Entity[], setAcceleration: (target: string, x: number, y:number, z:number) => void}) {
  function handleSubmit(ship: Entity) : (event: React.FormEvent<HTMLFormElement>) => void {
    return (event: React.FormEvent<HTMLFormElement>) => {
      console.log("Setting acceleration for " + ship.name);
      event.preventDefault();
      let x = Number(event.currentTarget.x.value);
      let y = Number(event.currentTarget.y.value);
      let z = Number(event.currentTarget.z.value);
      args.setAcceleration(ship.name, x, y, z);
    }
  }
  return (
    <>
    {args.entities.map((entity) => (
        <form key={entity.name + "-accel-setter"} className="as-form" onSubmit={handleSubmit(entity)}>
          <label className="as-label">{entity.name}</label>
          <div >
          <input className="as-input" name="x" type="text" defaultValue={0} />
          <input className="as-input" name="y" type="text" defaultValue={0} />
          <input className="as-input" name="z" type="text" defaultValue={0} />
          <input className="as-input as-button" type="submit" value="Set"/>
          </div>
        </form>
    ))}
    </>
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
        Number(addShip.xpos),
        Number(addShip.ypos),
        Number(addShip.zpos),
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
    };
    console.log("Adding ship: " + JSON.stringify(newShip));

    args.submitHandler(newShip);
    addShipUpdate(initialShip);
  }

  return (
    <form onSubmit={handleSubmit}>
      <title>Add Ship</title>
      <label className="control-label">Name</label>
      <input
        className="control-name-input"
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
      <input className="control-input control-button" type="submit" value="Create Ship" />
    </form>
  );
}

function Controls(args: {
  nextRound: (callback: EntityRefreshCallback) => void;
  getEntities: (entities: Entity[]) => void;
  addEntity: (entity: Entity, callback: EntityRefreshCallback) => void;
  setAcceleration: (target: string, acceleration: [number, number, number], callBack: (entities: Entity[]) => void) => void;
}) {
  const entityUpdater = useContext(EntitiesServerUpdateContext);
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <div className="controls-pane">
      <h1 className="control-label">Controls</h1>
      <AddShip submitHandler={(entity) => args.addEntity(entity, args.getEntities)} />
      <AccelerationManager entities={serverEntities} setAcceleration={(target, x, y, z) => {args.setAcceleration(target, [x, y, z], args.getEntities)}} />
      <button
        className="control-input control-button"
        onClick={() => args.nextRound(args.getEntities)}>
        Next Round
      </button>
    </div>
  );
}

export default Controls;
