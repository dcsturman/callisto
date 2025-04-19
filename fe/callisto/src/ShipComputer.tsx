import React from "react";
import {useState, useEffect, useContext, useMemo} from "react";
import {
  FlightPathResult,
  Ship,
  EntitiesServerContext,
  DEFAULT_ACCEL_DURATION,
  Acceleration,
  POSITION_SCALE,
  ViewContext,
  ViewMode,
} from "./Universal";

import {setPlan, setCrewActions} from "./ServerManager";
import {ActionContext, SensorState, SensorAction, newSensorState} from "./Actions";
import {EntitySelectorType, EntitySelector} from "./EntitySelector";

// Distance in km for standoff from another ship.
const DEFAULT_SHIP_STANDOFF_DISTANCE: number = 10;

type ShipComputerProps = {
  ship: Ship;
  setComputerShipName: (ship_name: string | null) => void;
  proposedPlan: FlightPathResult | null;
  getAndShowPlan: (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null,
    target_accel: [number, number, number] | null,
    standoff: number
  ) => void;
  sensorLocks: string[];
};

export const ShipComputer: React.FC<ShipComputerProps> = ({
  ship,
  setComputerShipName,
  proposedPlan,
  getAndShowPlan,
  sensorLocks,
}) => {
  const viewContext = useContext(ViewContext);
  const serverEntities = useContext(EntitiesServerContext).entities;

  const initNavigationTargetState = useMemo(() => {
    return {
      p_x: 0.0,
      p_y: 0.0,
      p_z: 0.0,
      v_x: 0.0,
      v_y: 0.0,
      v_z: 0.0,
      a_x: 0.0,
      a_y: 0.0,
      a_z: 0.0,
      standoff: 0.0,
    };
  }, []);

  // Its important to differentiate the following two similar states.
  // CurrentNavTarget is the entity currently being used as the navigation target.
  // navigationTarget holds the raw coordinates of a navigation target.  So
  // when currentNavTarget changes so will navigationTarget.  However, the position, velocity, standoff
  // of navigationTarget can then be changed to tweak/alter the navigation target.
  const [currentNavTarget, setCurrentNavTarget] = useState<string | null>(null);
  const [navigationTarget, setNavigationTarget] = useState(initNavigationTargetState);

  useEffect(() => {
    if (currentNavTarget == null) {
      setNavigationTarget(initNavigationTargetState);
      return;
    }

    if (currentNavTarget === ship.name) {
      setNavigationTarget(initNavigationTargetState);
      setCurrentNavTarget(null);
      return;
    }

    let standoff = DEFAULT_SHIP_STANDOFF_DISTANCE;

    const planet = serverEntities.planets.find((planet) => planet.name === currentNavTarget);

    if (planet) {
      standoff = (planet.radius * 1.1) / POSITION_SCALE;
    }

    const target_ship: Ship | undefined = serverEntities.ships.find(
      (ship) => ship.name === currentNavTarget
    );
    const entity = planet || target_ship;

    if (entity == null) {
      console.error(`(ShipComputer) Unable to find entity ${currentNavTarget}`);
      return;
    }

    setNavigationTarget({
      p_x: entity.position[0],
      p_y: entity.position[1],
      p_z: entity.position[2],
      v_x: entity.velocity[0],
      v_y: entity.velocity[1],
      v_z: entity.velocity[2],
      a_x: target_ship ? target_ship.plan[0][0][0] : 0.0,
      a_y: target_ship ? target_ship.plan[0][0][1] : 0.0,
      a_z: target_ship ? target_ship.plan[0][0][2] : 0.0,
      standoff,
    });

    // Also implicitly compute a plan since most of the time this is what the user wants.
    getAndShowPlan(
      ship.name,
      entity.position,
      entity.velocity,
      entity.velocity,
      target_ship ? target_ship.plan[0][0] : null,
      standoff
    );
  }, [currentNavTarget, serverEntities, ship.name, initNavigationTargetState, getAndShowPlan]);

  // Used only in the agility setting control, but that control isn't technically a React component
  // so need to define this here.
  const assistGunners = useMemo(() => ship.assist_gunners, [ship]);
  const agility = useMemo(() => ship.dodge_thrust, [ship]);

  const startAccel = [ship?.plan[0][0][0], ship?.plan[0][0][1], ship?.plan[0][0][2]];

  // TODO: Consider making all this unsupervised to make entry easier.
  const [computerAccel, setComputerAccel] = useState({
    x: startAccel[0],
    y: startAccel[1],
    z: startAccel[2],
  });

  // This is where we convert from string back into number, and thus
  // we only do this precision-losing conversion when a human enters a new value.
  // We do not do such conversions based on values from the server.
  function handleNavigationChange(event: React.ChangeEvent<HTMLInputElement>) {
    if (event.target.name === "p_x" || event.target.name === "p_y" || event.target.name === "p_z") {
      setNavigationTarget({
        ...navigationTarget,
        [event.target.name]: Number(event.target.value) * POSITION_SCALE,
      });
    } else {
      setNavigationTarget({
        ...navigationTarget,
        [event.target.name]: Number(event.target.value),
      });
    }
  }

  function handleNavigationSubmit(event: React.FormEvent<HTMLFormElement>) {
    // Perform computation logic here
    event.preventDefault();

    const end_pos: [number, number, number] = [
      navigationTarget.p_x,
      navigationTarget.p_y,
      navigationTarget.p_z,
    ];
    const end_vel: [number, number, number] = [
      navigationTarget.v_x,
      navigationTarget.v_y,
      navigationTarget.v_z,
    ];
    const target_vel: [number, number, number] | null = [
      navigationTarget.v_x,
      navigationTarget.v_y,
      navigationTarget.v_z,
    ];

    const target_accel: [number, number, number] | null = [
      navigationTarget.a_x,
      navigationTarget.a_y,
      navigationTarget.a_z,
    ];

    // TODO: Get rid of POSITION_SCALE and move to display
    const standoff = navigationTarget.standoff * POSITION_SCALE;

    console.log(
      `Computing route for ${ship.name} to ${end_pos} ${end_vel} with target velocity ${target_vel} target acceleration ${target_accel} with standoff ${standoff}`
    );

    // Called directly - usually when the user has specifically modified the values.
    // Can also be called implicitly in handleNavTargetSelect
    getAndShowPlan(ship.name, end_pos, end_vel, target_vel, target_accel, standoff);
  }

  function handleAssignPlan() {
    if (proposedPlan == null) {
      console.error(`(Controls.handleAssignPlan) No current plan`);
    } else {
      setComputerAccel({
        x: proposedPlan.plan[0][0][0],
        y: proposedPlan.plan[0][0][1],
        z: proposedPlan.plan[0][0][2],
      });
      setPlan(ship.name, proposedPlan.plan);
    }
  }

  // Intentionally defining as a function that returns JSX vs a true component.  If I use a true component then
  // we lose focus on each key stroke.  But I do need accelerationManager nested inside ShipComputer as we want to share
  // the computerAccel state between this component and the navigation computer functionality.
  function accelerationManager(): JSX.Element {
    function handleSetAcceleration(event: React.FormEvent<HTMLFormElement>) {
      event.preventDefault();
      const x = computerAccel.x;
      const y = computerAccel.y;
      const z = computerAccel.z;

      setPlan(ship.name, [[[x, y, z], DEFAULT_ACCEL_DURATION], null]);
    }

    function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
      setComputerAccel({
        ...computerAccel,
        [event.target.name]: Number(event.target.value),
      });
    }

    return (
      <>
        <h2 className="control-form">Set Accel (G&apos;s)</h2>
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
          <input className="control-input control-button blue-button" type="submit" value="Set" />
        </form>
      </>
    );
  }

  function pilotActions(): JSX.Element {
    function handleCrewActionChange(dodge: number, assist: boolean) {
      if (dodge === undefined) {
        dodge = 0;
      }
      ship.dodge_thrust = dodge;
      ship.assist_gunners = assist;
      setCrewActions(ship.name, dodge, assist);
    }
    return (
      <>
        <h2 className="control-form">Pilot Actions</h2>
        <div id="crew-actions-form" className="control-form">
          <div className="crew-actions-form-container">
            <label className="control-label">Dodge</label>
            <input
              className="control-input"
              type="text"
              value={agility.toString()}
              onChange={(event) =>
                handleCrewActionChange(Number(event.target.value), assistGunners)
              }
            />
            <label className="control-label">Assist Gunner</label>
            <input
              type="checkbox"
              checked={assistGunners}
              onChange={() => handleCrewActionChange(agility, !assistGunners)}
            />
          </div>
        </div>
      </>
    );
  }

  const title = ship.name + " Controls";

  // TODO: Full Stop is not correct, but needs server-side functions.  Should just get to 0 velocity and not care about position.
  // Current version tries to stop at the current position.
  return (
    <div id="computer-window" className="computer-window">
      {viewContext.role === ViewMode.General && <h1>{title}</h1>}
      {[ViewMode.General, ViewMode.Pilot].includes(viewContext.role) && pilotActions()}
      {[ViewMode.General, ViewMode.Sensors].includes(viewContext.role) && (
        <SensorActionChooser ship={ship} sensorLocks={sensorLocks} />
      )}
      <hr />
      {[ViewMode.General, ViewMode.Pilot].includes(viewContext.role) && (
        <>
          {accelerationManager()}
          <hr />
          <button
            className="control-input control-button blue-button"
            onClick={() => {
              setNavigationTarget({
                p_x: ship.position[0],
                p_y: ship.position[1],
                p_z: ship.position[2],
                v_x: 0,
                v_y: 0,
                v_z: 0,
                a_x: 0,
                a_y: 0,
                a_z: 0,
                standoff: 0,
              });
              getAndShowPlan(ship.name, ship.position, [0, 0, 0], null, null, 0);
            }}>
            Full Stop
          </button>
          <hr />
          <h2 className="control-form">Navigation</h2>
          <form className="target-entry-form" onSubmit={handleNavigationSubmit}>
            <label className="control-label" style={{display: "flex"}}>
              Nav Target:
              <EntitySelector
                filter={[EntitySelectorType.Ship, EntitySelectorType.Planet]}
                current={currentNavTarget}
                setChoice={(entity) => setCurrentNavTarget(entity?.name ?? null)}
                exclude={ship.name}
              />
            </label>
            <div className="target-details-div">
              <label className="control-label">
                Target Position (km)
                <div style={{display: "flex"}} className="coordinate-input">
                  <input
                    className="control-input"
                    name="p_x"
                    type="text"
                    value={(navigationTarget.p_x / POSITION_SCALE).toFixed(0)}
                    onChange={handleNavigationChange}
                  />
                  <input
                    className="control-input"
                    name="p_y"
                    type="text"
                    value={(navigationTarget.p_y / POSITION_SCALE).toFixed(0)}
                    onChange={handleNavigationChange}
                  />
                  <input
                    className="control-input"
                    name="p_z"
                    type="text"
                    value={(navigationTarget.p_z / POSITION_SCALE).toFixed(0)}
                    onChange={handleNavigationChange}
                  />
                </div>
              </label>
              <label className="control-label">
                Target Velocity (m/s)
                <div style={{display: "flex"}} className="coordinate-input">
                  <input
                    className="control-input"
                    name="v_x"
                    type="text"
                    value={navigationTarget.v_x.toFixed(1)}
                    onChange={handleNavigationChange}
                  />
                  <input
                    className="control-input"
                    name="v_y"
                    type="text"
                    value={navigationTarget.v_y.toFixed(1)}
                    onChange={handleNavigationChange}
                  />
                  <input
                    className="control-input"
                    name="v_z"
                    type="text"
                    value={navigationTarget.v_z.toFixed(1)}
                    onChange={handleNavigationChange}
                  />
                </div>
              </label>
              <label className="control-label">
                <span>Target Accel (G&apos;s)</span>
                <div style={{display: "flex"}} className="coordinate-input">
                  <input
                    className="control-input"
                    name="a_x"
                    type="text"
                    value={navigationTarget.a_x.toFixed(1)}
                    onChange={handleNavigationChange}
                  />
                  <input
                    className="control-input"
                    name="a_y"
                    type="text"
                    value={navigationTarget.a_y.toFixed(1)}
                    onChange={handleNavigationChange}
                  />
                  <input
                    className="control-input"
                    name="a_z"
                    type="text"
                    value={navigationTarget.a_z.toFixed(1)}
                    onChange={handleNavigationChange}
                  />
                </div>
              </label>
              <label
                className="control-label"
                style={{display: "flex", justifyContent: "space-between"}}>
                Standoff (km)
                <div className="coordinate-input">
                  <input
                    className="control-input standoff-input"
                    name="standoff"
                    type="text"
                    value={navigationTarget.standoff.toFixed(0)}
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
          {proposedPlan && (
            <div>
              <h2 className="control-form">Proposed Plan</h2>
              <NavigationPlan plan={proposedPlan.plan} />
              <button
                className="control-input control-button blue-button"
                onClick={handleAssignPlan}>
                Assign Plan
              </button>
            </div>
          )}
        </>
      )}
      {viewContext.role === ViewMode.General && !viewContext.shipName && (
        <button
          className="control-input control-button blue-button"
          onClick={() => {
            getAndShowPlan(null, [0, 0, 0], [0, 0, 0], null, null, 0);
            setComputerShipName(null);
          }}>
          Close
        </button>
      )}
    </div>
  );
};

function sensorActionToString(action: SensorState): string {
  switch (action.action) {
    case SensorAction.None:
      return "none";
    case SensorAction.JamMissiles:
      return "jam-missiles";
    case SensorAction.BreakSensorLock:
      return "bsl-" + action.target;
    case SensorAction.SensorLock:
      return "sl-" + action.target;
    case SensorAction.JamComms:
      return "jc-" + action.target;
  }
}

interface SensorActionChooserProps {
  ship: Ship;
  sensorLocks: string[];
}

const SensorActionChooser: React.FC<SensorActionChooserProps> = ({ship, sensorLocks}) => {
  const actionContext = useContext(ActionContext);

  const currentSensor = useMemo(() => {
    return actionContext.actions[ship.name]?.sensor || newSensorState(SensorAction.None, "");
  }, [actionContext.actions, ship.name]);

  function handleSensorActionChange(event: React.ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;
    if (value === "none") {
      actionContext.setSensorAction(ship.name, newSensorState(SensorAction.None, ""));
      return;
    } else if (value === "jam-missiles") {
      actionContext.setSensorAction(ship.name, newSensorState(SensorAction.JamMissiles, ""));
    } else if (value.startsWith("bsl-")) {
      actionContext.setSensorAction(
        ship.name,
        newSensorState(SensorAction.BreakSensorLock, value.substring(4))
      );
    } else if (value.startsWith("sl-")) {
      actionContext.setSensorAction(
        ship.name,
        newSensorState(SensorAction.SensorLock, value.substring(3))
      );
    } else if (value.startsWith("jc-")) {
      actionContext.setSensorAction(
        ship.name,
        newSensorState(SensorAction.JamComms, value.substring(3))
      );
    }
  }
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <div className="control-label">
      <h2 className="control-label">Sensor Actions</h2>
      <select
        className="sensor-action-select control-input "
        value={sensorActionToString(currentSensor)}
        onChange={handleSensorActionChange}>
        <option value="none"></option>
        <option value="jam-missiles">Jam Missiles</option>
        {sensorLocks.map((s) => (
          <option key={s + "-break-sensor-lock"} value={"bsl-" + s}>
            {"Break Sensor Lock: " + s}
          </option>
        ))}
        {serverEntities.entities.ships
          .filter(
            (s) =>
              s.name !== ship.name &&
              !serverEntities.entities.ships
                .find((s) => ship.name === s.name)
                ?.sensor_locks.includes(s.name)
          )
          .map((s) => (
            <option key={s.name + "-sensor-lock"} value={"sl-" + s.name}>
              {"Sensor Lock: " + s.name}
            </option>
          ))}

        {serverEntities.entities.ships
          .filter((s) => s.name !== ship.name)
          .map((s) => (
            <option key={s.name + "-jam-comms"} value={"jc-" + s.name}>
              {"Jam Sensors: " + s.name}
            </option>
          ))}
      </select>
      {ship.sensor_locks.length > 0 && (
        <>
          <div className="control-label">
            <h3 className="control-label">Sensor Locks</h3>
            <span className="plan-accel-text">
              {" "}
              {serverEntities.entities.ships
                .find((s) => ship.name === s.name)
                ?.sensor_locks.join(", ")}
            </span>
          </div>
        </>
      )}
    </div>
  );
};

export function NavigationPlan(args: {plan: [Acceleration, Acceleration | null]}) {
  function prettyPrintAccel(accel: Acceleration) {
    // explicitly round down acceleration so if user is copy/pasting they
    // don't get an "acceleration too high" error.
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
