import React from "react";
import { useContext, useState, useRef } from "react";
import {
  EntitiesServerContext,
  FlightPathResult,
  Ship,
  Planet,
  Entity,
  DEFAULT_ACCEL_DURATION,
  Acceleration,
  POSITION_SCALE,
} from "./Universal";

import { setPlan, setCrewActions } from "./ServerManager";

import { EntitySelectorType, EntitySelector } from "./EntitySelector";

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
      "Buccaneer",
      0,
      0,
      0,
      0,
      0,
      0,
      0,
      "",
      [],
      0,
      false
    );

  if (ship == null) {
    console.error(
      `(ShipComputer) Unable to find ship of name "${args.shipName}!`
    );
  }

  // Used only in the agility setting control, but that control isn't technically a React component
  // so need to define this here.
  const [agility, setDodge] = useState(ship.dodge_thrust);
  const [assistGunners, setAssistGunners] = useState(ship.assist_gunners);

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

  const startAccel = [
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

    const end_pos: [number, number, number] = [
      Number(navigationTarget.p_x) * POSITION_SCALE,
      Number(navigationTarget.p_y) * POSITION_SCALE,
      Number(navigationTarget.p_z) * POSITION_SCALE,
    ];
    const end_vel: [number, number, number] = [
      Number(navigationTarget.v_x),
      Number(navigationTarget.v_y),
      Number(navigationTarget.v_z),
    ];
    const target_vel: [number, number, number] | null = [
      Number(navigationTarget.v_x),
      Number(navigationTarget.v_y),
      Number(navigationTarget.v_z),
    ];

    const standoff = Number(navigationTarget.standoff) * POSITION_SCALE;

    console.log(
      `Computing route for ${ship.name} to ${end_pos} ${end_vel} with target velocity ${target_vel} with standoff ${standoff}`
    );
    args.getAndShowPlan(ship.name, end_pos, end_vel, target_vel, standoff);
  }

  function handleNavTargetSelect(entity: Entity | null) {
    if (entity == null) {
      setNavigationTarget(initNavigationTargetState);
      return;
    }

    const p_x = (entity.position[0] / POSITION_SCALE).toFixed(0);
    const p_y = (entity.position[1] / POSITION_SCALE).toFixed(0);
    const p_z = (entity.position[2] / POSITION_SCALE).toFixed(0);
    const v_x = entity.velocity[0].toFixed(1);
    const v_y = entity.velocity[1].toFixed(1);
    const v_z = entity.velocity[2].toFixed(1);
    const standoff = (
      entity instanceof Ship
        ? 1000
        : ((entity as Planet).radius * 1.1) / POSITION_SCALE
    ).toFixed(1);

    setNavigationTarget({
      p_x,
      p_y,
      p_z,
      v_x,
      v_y,
      v_z,
      standoff,
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
      setPlan(ship.name, args.proposedPlan.plan).then(() =>
        args.resetProposedPlan()
      );

      if (selectRef.current !== null) {
        selectRef.current.value = "";
      }

      setNavigationTarget(initNavigationTargetState);
    }
  }

  // Intentionally defining as a function that returns JSX vs a true component.  If I use a true component then
  // we lose focus on each key stroke.  But I do need accelerationManager nested inside ShipComputer as we want to share
  // the computerAccel state between this component and the navigation computer functionality.
  function accelerationManager(): JSX.Element {
    function handleSetAcceleration(event: React.FormEvent<HTMLFormElement>) {
      event.preventDefault();
      const x = Number(computerAccel.x);
      const y = Number(computerAccel.y);
      const z = Number(computerAccel.z);

      const setColor = (id: string, color: string) => {
        const elem = document.getElementById(id);
        if (elem !== null) {
          elem.style.color = color;
        }
      };

      setPlan(ship.name, [[[x, y, z], DEFAULT_ACCEL_DURATION], null])
        .then(() => {
          setColor("control-input-x", "black");
          setColor("control-input-y", "black");
          setColor("control-input-z", "black");

          args.resetProposedPlan();
        })
        .catch(() => {
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
        <h2 className="control-form">
          Set Accel (m/s<sup>2</sup>)
        </h2>
        <form
          key={ship.name + "-accel-setter"}
          className="as-form"
          onSubmit={handleSetAcceleration}>
          <input
            className="control-input"
            id="control-input-x"
            name="x"
            type="text"
            onChange={handleChange}
            value={computerAccel.x}
          />
          <input
            className="control-input"
            id="control-input-y"
            name="y"
            type="text"
            onChange={handleChange}
            value={computerAccel.y}
          />
          <input
            className="control-input"
            id="control-input-z"
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

  function crewActions(): JSX.Element {
    function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
      event.preventDefault();
      ship.dodge_thrust = agility;
      setCrewActions(ship.name, agility, assistGunners);
    }

    return (
      <>
        <h2 className="control-form">Crew Actions</h2>
        <form
          id="crew-actions-form"
          className="control-form"
          onSubmit={handleSubmit}>
          <div className="crew-actions-form-container">
            <label className="control-label">Dodge</label>
            <input
              className="control-input"
              type="text"
              value={agility.toString()}
              onChange={(event) => setDodge(Number(event.target.value))}
            />

            <label className="control-label">Assist Gunner</label>
            <input
              type="checkbox"
              checked={assistGunners}
              onChange={() => setAssistGunners(!assistGunners)}
            />
          </div>
          <input
            className="control-input control-button blue-button"
            type="submit"
            value="Set"
          />
        </form>
      </>
    );
  }

  const title = ship.name + " Nav";

  return (
    <div id="computer-window" className="computer-window">
      <h1>{title}</h1>
      {crewActions()}
      <hr />
      {accelerationManager()}
      <hr />
      <button
        className="control-input control-button blue-button"
        onClick={() => {
          setNavigationTarget({
            p_x: (ship.position[0] / POSITION_SCALE).toString(),
            p_y: (ship.position[1] / POSITION_SCALE).toString(),
            p_z: (ship.position[2] / POSITION_SCALE).toString(),
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
          <EntitySelector filter={[EntitySelectorType.Ship, EntitySelectorType.Planet]} onChange={handleNavTargetSelect} exclude={ship.name} />
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

export function NavigationPlan(args: {
  plan: [Acceleration, Acceleration | null];
}) {
  function prettyPrintAccel(accel: Acceleration) {
    const ax = accel[0][0].toFixed(2).padStart(5, " ");
    const ay = accel[0][1].toFixed(2).padStart(6, " ");
    const az = accel[0][2].toFixed(2).padStart(6, " ");
    const time = accel[1].toFixed(0).padStart(4, " ");
    const s = `${time}s @ (${ax},${ay},${az})`;
    return s;
  }

  const accel0 = args.plan[0];
  const accel1 = args.plan[1];

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
