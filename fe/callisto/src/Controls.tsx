import { useContext, useState, useEffect, useRef } from "react";
import * as React from "react";
import { Tooltip } from "react-tooltip";
import * as THREE from "three";
import { Crew } from "./CrewBuilder";
import { Accordion } from "./Accordion";

import {
  EntitiesServerContext,
  EntityRefreshCallback,
  Ship,
  ViewControlParams,
  Entity,
  Planet,
  ShipDesignTemplates,
  Weapon,
  WeaponMount,
  POSITION_SCALE,
  SCALE,
} from "./Universal";

import { CrewBuilder } from "./CrewBuilder";
import { addShip } from "./ServerManager";
import { EntitySelector, EntitySelectorType } from "./EntitySelector";
import {
  scaleVector,
  vectorToString,
  findRangeBand,
  vectorDistance,
} from "./Util";
import { NavigationPlan } from "./ShipComputer";
import { WeaponButton, FireActions } from "./WeaponUse";

import { CiCircleQuestion } from "react-icons/ci";

class FireAction {
  target: string;
  weapon_id: number;
  called_shot_system: string | null;
  constructor(target: string, weapon_id: number) {
    this.weapon_id = weapon_id;
    this.target = target;
    this.called_shot_system = null;
  }
}

export type FireState = FireAction[];
export type FireActionMsg = { [key: string]: FireState };

function ShipList(args: {
  computerShip: Ship | null;
  setComputerShip: (ship: Ship | null) => void;
  setCameraPos: (pos: THREE.Vector3) => void;
  camera: THREE.Camera | null;
}) {
  function moveCameraToShip() {
    if (args.camera == null) {
      console.log("Cannot move camera because camera object in Three is null.");
      return;
    }
    if (args.computerShip) {
      const downCamera = new THREE.Vector3(0, 0, 40);
      downCamera.applyQuaternion(args.camera.quaternion);
      const new_camera_pos = new THREE.Vector3(
        args.computerShip.position[0] * SCALE,
        args.computerShip.position[1] * SCALE,
        args.computerShip.position[2] * SCALE
      ).add(downCamera);
      args.setCameraPos(new_camera_pos);
    }
  }

  return (
    <div className="control-launch-div">
      <h2 className="ship-list-label">Ship: </h2>
      <EntitySelector
        filter={[EntitySelectorType.Ship]}
        onChange={(entity) => args.setComputerShip(entity as Ship)}
        current={args.computerShip}
      />
      <button className="control-input blue-button" onClick={moveCameraToShip}>
        Go
      </button>
    </div>
  );
}

function ShipDesignList(args: {
  shipDesignName: string;
  setShipDesignName: (designName: string) => void;
  shipDesigns: ShipDesignTemplates;
}) {
  const selectRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (selectRef.current != null) {
      selectRef.current.value =
        (args.shipDesignName && args.shipDesignName) || "";
    }
  }, [args.shipDesignName]);

  function handleDesignListSelectChange(
    event: React.ChangeEvent<HTMLSelectElement>
  ) {
    const value = event.target.value;
    args.setShipDesignName(value);
  }

  function shipDesignDetails(render: {
    content: string | null;
    activeAnchor: HTMLElement | null;
  }) {
    if (render.content == null) {
      return <></>;
    }
    const design = args.shipDesigns[render.content];
    if (design == null) {
      return <>Select a ship design.</>;
    }

    const compressed = Object.values(design.compressedWeapons());
    const describeWeapon = (weapon: {
      kind: string;
      mount: WeaponMount;
      used: number;
      total: number;
    }) => {
      const weapon_name = new Weapon(weapon.kind, weapon.mount).toString();

      const [quant, suffix] =
        weapon.total === 1 ? ["a", ""] : [weapon.total, "s"];
      return `${quant} ${weapon_name}${suffix}`;
    };

    let weaponDesc = compressed.slice(0, -1).map((...[weapon]) => {
      return describeWeapon(weapon) + ", ";
    });

    if (compressed.length === 0) {
      weaponDesc = ["This ship is unarmed."];
    } else if (compressed.length === 1) {
      weaponDesc = ["Weapons are ", describeWeapon(compressed[0])];
    } else {
      weaponDesc.push(
        "and " + describeWeapon(compressed[compressed.length - 1])
      );
      weaponDesc = ["Weapons are "].concat(weaponDesc);
    }
    return (
      <>
        <h3>{design.name}</h3>
        <div className="ship-design-description-tooltip">
          {design.displacement} tons with {design.hull} hull points and{" "}
          {design.armor} armor.&nbsp;
          {design.power} power back {design.maneuver}G thrust and jump{" "}
          {design.jump}. {weaponDesc}.
        </div>
      </>
    );
  }
  return (
    <>
      <div className="control-launch-div">
        <div className="control-label">
          <div className="control-label label-with-tooltip">
            Design
            <CiCircleQuestion className="info-icon" />
          </div>
        </div>
        <select
          className="select-dropdown control-name-input control-input"
          name="ship_list_choice"
          ref={selectRef}
          defaultValue={args.shipDesignName || ""}
          onChange={handleDesignListSelectChange}
          data-tooltip-id={args.shipDesignName + "ship-description-tip"}
          data-tooltip-content={args.shipDesignName}
          data-tooltip-delay-show={700}>
          {Object.values(args.shipDesigns)
            .sort((a, b) =>
              a.displacement > b.displacement
                ? 1
                : a.displacement < b.displacement
                ? -1
                : a.name.localeCompare(b.name)
            )
            .map((design) => (
              <option
                key={design.name + "-ship_list"}
                value={
                  design.name
                }>{`${design.name} (${design.displacement})`}</option>
            ))}
        </select>
        <Tooltip
          id={args.shipDesignName + "ship-description-tip"}
          className="tooltip-body"
          render={shipDesignDetails}
        />
      </div>
      <Tooltip
        id="design-tooltip"
        anchorSelect=".info-icon"
        content="Select the design of the ship you wish to add to the scenario."
      />
    </>
  );
}

function AddShip(args: {
  submitHandler: (
    name: string,
    position: [number, number, number],
    velocity: [number, number, number],
    acceleration: [number, number, number],
    design: string,
    crew: Crew
  ) => void;
  shipDesignTemplates: ShipDesignTemplates;
}) {
  const designRef = useRef<HTMLInputElement>(null);

  const initialShip = {
    name: "ShipName",
    xpos: "0",
    ypos: "0",
    zpos: "0",
    xvel: "0",
    yvel: "0",
    zvel: "0",
    design: Object.values(args.shipDesignTemplates)[0].name,
    crew: new Crew(),
  };

  const [addShip, addShipUpdate] = useState(initialShip);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    if (designRef.current) {
      designRef.current.style.color = "black";
    }

    addShipUpdate({ ...addShip, [event.target.name]: event.target.value });
  }

  function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = addShip.name;
    const position: [number, number, number] = [
      Number(addShip.xpos) * POSITION_SCALE,
      Number(addShip.ypos) * POSITION_SCALE,
      Number(addShip.zpos) * POSITION_SCALE,
    ];
    const velocity: [number, number, number] = [
      Number(addShip.xvel),
      Number(addShip.yvel),
      Number(addShip.zvel),
    ];

    const design: string = addShip.design;
    addShipUpdate({ ...addShip, design: design });

    const crew = addShip.crew;
    console.log(
      `Adding Ship ${name}: Position ${position}, Velocity ${velocity}, Design ${design}`
    );

    args.submitHandler(name, position, velocity, [0, 0, 0], design, crew);
    addShipUpdate(initialShip);
  }

  return (
    <Accordion id="add-ship-header" title="Add Ship" initialOpen={false}>
      <form id="add-ship" className="control-form" onSubmit={handleSubmit}>
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
        <ShipDesignList
          shipDesignName={addShip.design}
          setShipDesignName={(design) =>
            addShipUpdate({ ...addShip, design: design })
          }
          shipDesigns={args.shipDesignTemplates}
        />
        <hr />
        <CrewBuilder
          shipName={addShip.name}
          updateCrew={(crew: Crew) => addShipUpdate({ ...addShip, crew: crew })}
          shipDesign={args.shipDesignTemplates[addShip.design]}
        />
        <input
          className="control-input control-button blue-button"
          type="submit"
          value="Add"
        />
      </form>
    </Accordion>
  );
}

export function Controls(args: {
  nextRound: (
    fireActions: { [key: string]: FireState },
    callback: EntityRefreshCallback
  ) => void;
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
}) {
  // fire_actions is, for each ship, all weapons grouped together by kind and mount.
  // This allows them to be displayed as a single button with a count, and
  // track how many are used.
  const [fire_actions, setFireActions] = useState(
    {} as {
      [actor: string]: {
        weapons: {
          [weapon: string]: {
            kind: string;
            mount: WeaponMount;
            used: number;
            total: number;
          };
        };
        state: FireState;
      };
    }
  );

  const [fireTarget, setFireTarget] = useState<Entity | null>(null);

  const serverEntities = useContext(EntitiesServerContext);

  const computerShipDesign = args.computerShip
    ? args.shipDesignTemplates[args.computerShip.design]
    : null;

  if (
    computerShipDesign &&
    args.computerShip &&
    !fire_actions[args.computerShip.name]
  ) {
    const compressed_weapons = computerShipDesign.compressedWeapons();
    setFireActions({
      ...fire_actions,
      [args.computerShip.name]: { weapons: compressed_weapons, state: [] },
    });
  }

  function handleFireCommand(attacker: string, target: string, weapon: string) {
    if (!computerShipDesign) {
      console.error(
        "(Controls.handleFireCommand) No computer ship design for " +
          attacker +
          "."
      );
      return;
    }

    if (
      fire_actions[attacker]?.weapons[weapon]?.used ===
      fire_actions[attacker]?.weapons[weapon]?.total
    ) {
      console.log(
        "(Controls.handleFireCommand) No more weapons of type " +
          weapon +
          " for " +
          attacker +
          "."
      );
      return;
    }
    fire_actions[attacker].weapons[weapon].used += 1;

    let nth_weapon = fire_actions[attacker].weapons[weapon].used;
    let weapon_position = 0;
    for (
      ;
      weapon_position <
      args.shipDesignTemplates[computerShipDesign.name].weapons.length;
      weapon_position++
    ) {
      if (
        args.shipDesignTemplates[computerShipDesign.name].weapons[
          weapon_position
        ].toString() === weapon
      ) {
        nth_weapon -= 1;
        if (nth_weapon === 0) {
          break;
        }
      }
    }

    // Check error conditions out of that loop.
    if (
      weapon_position ===
        args.shipDesignTemplates[computerShipDesign.name].weapons.length ||
      nth_weapon !== 0
    ) {
      console.error(
        "(Controls.handleFireCommand) Could not find " +
          fire_actions[attacker].weapons[weapon].used +
          "th weapon " +
          weapon +
          " for " +
          attacker +
          "."
      );
      return;
    }

    const new_fire_action = new FireAction(target, weapon_position);
    setFireActions({
      ...fire_actions,
      [attacker]: {
        ...fire_actions[attacker],
        state: [...fire_actions[attacker].state, new_fire_action],
      },
    });
  }

  return (
    <div className="controls-pane">
      <h1>Controls</h1>
      <hr />
      {args.shipDesignTemplates &&
        Object.keys(args.shipDesignTemplates).length > 0 && (
          <AddShip
            submitHandler={(
              name: string,
              position: [number, number, number],
              velocity: [number, number, number],
              acceleration: [number, number, number],
              designName: string,
              crew: Crew
            ) =>
              addShip(name, position, velocity, acceleration, designName, crew)
            }
            shipDesignTemplates={args.shipDesignTemplates}
          />
        )}
      <hr />
      <Accordion id="ship-computer" title="Ship's Computer" initialOpen={true}>
        <ShipList
          computerShip={args.computerShip}
          setComputerShip={(ship) => {
            args.setShowRange(null);
            args.setComputerShip(ship);
            setFireTarget(null);
          }}
          setCameraPos={args.setCameraPos}
          camera={args.camera}
        />
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
              <div className="stats-bloc-entry">
                <h2>Sensors</h2>
                <pre className="plan-accel-text">
                  {args.computerShip.current_sensors}
                </pre>
              </div>
            </div>
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
            <div className="control-form">
              <label className="control-label">
                <h2>Fire Control</h2>
                <div className="control-launch-div">
                  Target:
                  <EntitySelector
                    filter={[EntitySelectorType.Ship]}
                    onChange={setFireTarget}
                    current={fireTarget}
                    exclude={args.computerShip.name}
                    formatter={(name, entity) => {
                      if (args.computerShip) {
                        return `${name} (${findRangeBand(
                          vectorDistance(
                            args.computerShip.position,
                            entity.position
                          )
                        )})`;
                      } else {
                        return "";
                      }
                    }}
                  />
                </div>
                <div className="weapon-list">
                  {fire_actions[args.computerShip.name] &&
                    Object.values(fire_actions[args.computerShip.name].weapons).map(
                      (weapon, id) =>
                        weapon.kind !== "Sand" && (
                          <WeaponButton
                            key={"weapon-" + args.computerShip?.name + "-" + id}
                            weapon={weapon.kind}
                            mount={weapon.mount}
                            count={weapon.total - weapon.used}
                            onClick={() => {
                              handleFireCommand(
                                args.computerShip ? args.computerShip.name : "",
                                fireTarget ? fireTarget.name : "",
                                new Weapon(weapon.kind, weapon.mount).toString()
                              );
                            }}
                            disable={!fireTarget}
                          />
                        )
                    )}
                </div>
              </label>
            </div>
          </>
        )}
        {args.computerShip &&
          computerShipDesign &&
          (fire_actions[args.computerShip.name]?.state || []).length > 0 && (
            <FireActions
              actions={fire_actions[args.computerShip?.name].state || []}
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
          args.nextRound(
            Object.entries(fire_actions).reduce((acc, [key, value]) => {
              return { ...acc, [key]: value.state };
            }, {} as { [key: string]: FireState }),
            serverEntities.handler
          );
          setFireActions({});
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
