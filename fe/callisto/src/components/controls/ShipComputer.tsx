import * as React from "react";
import {useState, useEffect, useMemo} from "react";
import {DEFAULT_ACCEL_DURATION, POSITION_SCALE} from "lib/universal";
import {Ship, Acceleration} from "lib/entities";
import {ViewMode} from "lib/view";

import {setPlan, setCrewActions} from "lib/serverManager";
import {SensorState, SensorAction, newSensorState} from "components/controls/Actions";
import {EntitySelectorType, EntitySelector} from "lib/EntitySelector";
import {findShip} from "lib/entities";

import {useAppSelector, useAppDispatch} from "state/hooks";
import {entitiesSelector} from "state/serverSlice";
import {setComputerShipName} from "state/uiSlice";
import {setSensorAction, jump} from "state/actionsSlice";
import {computeFlightPath} from "lib/serverManager";

// Distance in km for standoff from another ship.
const DEFAULT_SHIP_STANDOFF_DISTANCE: number = 10;

type ShipComputerProps = {
  ship: Ship;
  sensorLocks: string[];
};

export const ShipComputer: React.FC<ShipComputerProps> = ({ship, sensorLocks}) => {
  const entities = useAppSelector(entitiesSelector);
  const role = useAppSelector((state) => state.user.role);
  const shipName = useAppSelector((state) => state.user.shipName);
  const proposedPlan = useAppSelector((state) => state.ui.proposedPlan);
  const dispatch = useAppDispatch();

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

  const target = useMemo(() => {
    if (currentNavTarget == null) {
      return;
    }

    const planet = entities.planets.find((planet) => planet.name === currentNavTarget);
    if (planet) {
      return {position: planet.position, velocity: planet.velocity, radius: planet.radius};
    }
    const ship = findShip(entities, currentNavTarget);

    if (ship == null)
      return;

    return {position: ship.position, velocity: ship.velocity, plan: ship.plan};
  }, [entities, currentNavTarget]);

  useEffect(() => {
    if (target == null) {
      setNavigationTarget(initNavigationTargetState);
      return;
    }

    if (currentNavTarget === ship.name) {
      setNavigationTarget(initNavigationTargetState);
      setCurrentNavTarget(null);
      return;
    }

    let standoff = DEFAULT_SHIP_STANDOFF_DISTANCE;

    if ("radius" in target) {
      standoff = (target.radius! * 1.1) / POSITION_SCALE;
    }

    const plan = target.plan?? null;
    
    setNavigationTarget({
      p_x: target.position[0],
      p_y: target.position[1],
      p_z: target.position[2],
      v_x: target.velocity[0],
      v_y: target.velocity[1],
      v_z: target.velocity[2],
      a_x: plan ? plan[0][0][0] : 0.0,
      a_y: plan ? plan[0][0][1] : 0.0,
      a_z: plan ? plan[0][0][2] : 0.0,
      standoff,
    });


    // Also implicitly compute a plan since most of the time this is what the user wants.
    computeFlightPath(
      ship.name,
      target.position,
      target.velocity,
      target.velocity,
      plan ? plan[0][0] : null,
      standoff
    );
  }, [currentNavTarget, target]);

  // Used only in the agility setting control, but that control isn't technically a React component
  // so need to define this here.
  const assistGunners = useMemo(() => ship.assist_gunners, [ship]);
  const agility = useMemo(() => ship.dodge_thrust, [ship]);

  const startAccel = [ship?.plan[0][0][0], ship?.plan[0][0][1], ship?.plan[0][0][2]];

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

    // Called directly - usually when the user has specifically modified the values.
    // Can also be called implicitly in handleNavTargetSelect
    computeFlightPath(ship.name, end_pos, end_vel, target_vel, target_accel, standoff);
  }

  function handleAssignPlan() {
    if (proposedPlan == null) {
      console.error(`(Controls.handleAssignPlan) No current plan`);
    } else {
      (document.getElementById("set-accel-input-x") as HTMLInputElement).value =
        proposedPlan.plan[0][0][0].toString();
      (document.getElementById("set-accel-input-y") as HTMLInputElement).value =
        proposedPlan.plan[0][0][1].toString();
      (document.getElementById("set-accel-input-z") as HTMLInputElement).value =
        proposedPlan.plan[0][0][2].toString();

      setPlan(ship.name, proposedPlan.plan);
    }
  }

  function checkNumericInput(id: string): number {
    const element = document.getElementById(id) as HTMLInputElement;
    const value = Number(element.value);
    if (isNaN(value)) {
      window.alert(`Invalid input: '${element.value}' is not a number.`);
      element.value = "0";
      return 0;
    }
    return value;
  }
  // Intentionally defining as a function that returns JSX vs a true component.  If I use a true component then
  // we lose focus on each key stroke.  But I do need accelerationManager nested inside ShipComputer as we want to share
  // the computerAccel state between this component and the navigation computer functionality.
  function accelerationManager(): JSX.Element {
    function handleSetAcceleration(event: React.FormEvent<HTMLFormElement>) {
      event.preventDefault();

      const x = checkNumericInput("set-accel-input-x");
      const y = checkNumericInput("set-accel-input-y");
      const z = checkNumericInput("set-accel-input-z");

      setPlan(ship.name, [[[x, y, z], DEFAULT_ACCEL_DURATION], null]);
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
            id="set-accel-input-x"
            name="x"
            type="text"
            defaultValue={startAccel[0].toString()}
          />
          <input
            className="control-input"
            id="set-accel-input-y"
            name="y"
            type="text"
            defaultValue={startAccel[1].toString()}
          />
          <input
            className="control-input"
            id="set-accel-input-z"
            name="z"
            type="text"
            defaultValue={startAccel[2].toString()}
          />
          <input className="control-input control-button blue-button" type="submit" value="Set" />
        </form>
      </>
    );
  }

  function pilotActions(): JSX.Element {
    function handleCrewActionChange(dodge: number, assist: boolean) {
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

  function attemptJump() {
    const name = ship.name;
    if (window.confirm(`Are you sure you want to have ${name} attempt a jump?`)) {
      dispatch(jump(name));
    }
  }

  const title = ship.name + " Controls";

  // TODO: Full Stop is not correct, but needs server-side functions.  Should just get to 0 velocity and not care about position.
  // Current version tries to stop at the current position.
  return (
    <div id="computer-window" className="computer-window">
      <div id="crew-actions-window">
        {role === ViewMode.General && <h1>{title}</h1>}
        {[ViewMode.General, ViewMode.Pilot].includes(role) && pilotActions()}
        {[ViewMode.General, ViewMode.Sensors].includes(role) && (
          <SensorActionChooser ship={ship} sensorLocks={sensorLocks} />
        )}
        {[ViewMode.General, ViewMode.Pilot].includes(role) && (
          <button
            className="control-input control-button blue-button"
            disabled={!ship.can_jump}
            onClick={attemptJump}>
            Jump
          </button>
        )}
      </div>
      <hr />
      {[ViewMode.General, ViewMode.Pilot].includes(role) && (
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
              computeFlightPath(ship.name, ship.position, [0, 0, 0], null, null, 0);
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
            <div id="proposed-plan-region">
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
      {role === ViewMode.General && !shipName && (
        <button
          className="control-input control-button blue-button"
          onClick={() => {
            computeFlightPath(null, [0, 0, 0], [0, 0, 0], null, null, 0);
            dispatch(setComputerShipName(null));
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
  const actions = useAppSelector((state) => state.actions);
  const entities = useAppSelector(entitiesSelector);
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const dispatch = useAppDispatch();

  const currentSensor = useMemo(() => {
    if (!computerShipName || !actions[computerShipName]) {
      return newSensorState(SensorAction.None, "");
    }
    return actions[computerShipName].sensor;
  }, [actions, computerShipName]);

  function handleSensorActionChange(event: React.ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;
    if (value === "none") {
      dispatch(
        setSensorAction({shipName: ship.name, action: newSensorState(SensorAction.None, "")})
      );
      return;
    } else if (value === "jam-missiles") {
      dispatch(
        setSensorAction({shipName: ship.name, action: newSensorState(SensorAction.JamMissiles, "")})
      );
    } else if (value.startsWith("bsl-")) {
      dispatch(
        setSensorAction({
          shipName: ship.name,
          action: newSensorState(SensorAction.BreakSensorLock, value.substring(4)),
        })
      );
    } else if (value.startsWith("sl-")) {
      dispatch(
        setSensorAction({
          shipName: ship.name,
          action: newSensorState(SensorAction.SensorLock, value.substring(3)),
        })
      );
    } else if (value.startsWith("jc-")) {
      dispatch(
        setSensorAction({
          shipName: ship.name,
          action: newSensorState(SensorAction.JamComms, value.substring(3)),
        })
      );
    }
  }

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
        {entities.ships
          .filter(
            (s) =>
              s.name !== ship.name &&
              !entities.ships.find((s) => ship.name === s.name)?.sensor_locks.includes(s.name)
          )
          .map((s) => (
            <option key={s.name + "-sensor-lock"} value={"sl-" + s.name}>
              {"Sensor Lock: " + s.name}
            </option>
          ))}

        {entities.ships
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
              {entities.ships.find((s) => ship.name === s.name)?.sensor_locks.join(", ")}
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
