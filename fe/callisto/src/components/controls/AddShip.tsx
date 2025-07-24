import * as React from "react";
import {useState, useRef, useEffect, useMemo} from "react";
import {CrewBuilder, Crew, createCrew} from "components/controls/CrewBuilder";
import {POSITION_SCALE} from "lib/universal";
import {ShipDesignTemplates, compressedWeaponsFromTemplate} from "lib/shipDesignTemplates";
import {WeaponMount, createWeapon, weaponToString} from "lib/weapon";
import {Accordion} from "lib/Accordion";
import {Tooltip} from "react-tooltip";
import {CiCircleQuestion} from "react-icons/ci";
import {unique_ship_name} from "lib/shipnames";
import {Ship, defaultShip, findShip} from "lib/entities";

import {addShip} from "lib/serverManager";
import {useAppSelector} from "state/hooks";
import {entitiesSelector} from "state/serverSlice";

type AddShipProps = unknown;

export const AddShip: React.FC<AddShipProps> = () => {
  const entities = useAppSelector(entitiesSelector);
  const shipDesignTemplates = useAppSelector(state => state.server.templates);

  const shipNames = useMemo(() => entities.ships.map((ship: Ship) => ship.name), [entities.ships]);

  const designRef = useRef<HTMLSelectElement>(null);
  const shipNameRef = useRef<HTMLInputElement>(null);

  const initialTemplate = {
    name: unique_ship_name(entities),
    xpos: "0",
    ypos: "0",
    zpos: "0",
    xvel: "0",
    yvel: "0",
    zvel: "0",
    design: Object.values(shipDesignTemplates)[0].name,
    crew: createCrew(Object.values(shipDesignTemplates)[0].weapons.length),
  };

  const [addShipData, setAddShipData] = useState(initialTemplate);

  useEffect(() => {
    const current =
      entities.ships.find((ship) => ship.name === addShipData.name) || null;
    if (current != null) {
      const template = {
        name: current.name,
        xpos: (current.position[0] / POSITION_SCALE).toString(),
        ypos: (current.position[1] / POSITION_SCALE).toString(),
        zpos: (current.position[2] / POSITION_SCALE).toString(),
        xvel: current.velocity[0].toString(),
        yvel: current.velocity[1].toString(),
        zvel: current.velocity[2].toString(),
        design: current.design,
        crew: current.crew,
      };
      setAddShipData(template);
    }
  }, [addShipData.name, entities.ships]);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    if (designRef.current) {
      designRef.current.style.color = "black";
    }

    event.target.style.color = "black";
    if (event.target.name === "name") {
      if (shipNames.includes(event.target.value)) {
        event.target.style.color = "green";
        const ship = entities.ships.find((ship) => ship.name === event.target.value);
        if (ship != null) {
          setAddShipData({
            name: event.target.value,
            xpos: (ship.position[0] / POSITION_SCALE).toString(),
            ypos: (ship.position[1] / POSITION_SCALE).toString(),
            zpos: (ship.position[2] / POSITION_SCALE).toString(),
            xvel: ship.velocity[0].toString(),
            yvel: ship.velocity[1].toString(),
            zvel: ship.velocity[2].toString(),
            design: ship.design,
            crew: ship.crew,
          });
        }
      }
    }
    setAddShipData({...addShipData, [event.target.name]: event.target.value});
  }

  function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();

    const name = addShipData.name;
    const position: [number, number, number] = [
      Number(addShipData.xpos) * POSITION_SCALE,
      Number(addShipData.ypos) * POSITION_SCALE,
      Number(addShipData.zpos) * POSITION_SCALE,
    ];
    const velocity: [number, number, number] = [
      Number(addShipData.xvel),
      Number(addShipData.yvel),
      Number(addShipData.zvel),
    ];

    const design: string = addShipData.design;
    setAddShipData({...addShipData, design: design});

    const crew = addShipData.crew;
    const ship = findShip(entities, name) || defaultShip();

    const revision = {...ship, name, position, velocity, design, crew};

    addShip(revision);
    setAddShipData(initialTemplate);
    shipNameRef.current!.style.color = "black";
  }

  return (
    <Accordion id="add-ship-header" title="Add or Update Ship" initialOpen={false}>
      <form id="add-ship" className="control-form" onSubmit={handleSubmit}>
        <div id="add-ship-top-part">
          <label className="control-label">
            Name
            <input
              id="add-ship-name-input"
              className="control-name-input control-input"
              name="name"
              type="text"
              onChange={handleChange}
              value={addShipData.name}
              ref={shipNameRef}
            />
          </label>
          <label className="control-label">
            Position (km)
            <div className="coordinate-input">
              <input
                className="control-input"
                name="xpos"
                type="text"
                value={addShipData.xpos}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="ypos"
                type="text"
                value={addShipData.ypos}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="zpos"
                type="text"
                value={addShipData.zpos}
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
                value={addShipData.xvel}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="yvel"
                type="text"
                value={addShipData.yvel}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="zvel"
                type="text"
                value={addShipData.zvel}
                onChange={handleChange}
              />
            </div>
          </label>
          <ShipDesignList
            shipDesignName={addShipData.design}
            setShipDesignName={(design) => setAddShipData({...addShipData, design: design})}
            shipDesigns={shipDesignTemplates}
          />
        </div>
        <hr />
        <CrewBuilder
          shipName={addShipData.name}
          updateCrew={(crew: Crew) => setAddShipData({...addShipData, crew: crew})}
          shipDesign={shipDesignTemplates[addShipData.design]}
        />
        <input
          className="control-input control-button blue-button"
          type="submit"
          value={shipNames.includes(addShipData.name) ? "Update" : "Add"}
        />
      </form>
    </Accordion>
  );
};

function ShipDesignList(args: {
  shipDesignName: string;
  setShipDesignName: (designName: string) => void;
  shipDesigns: ShipDesignTemplates;
}) {
  const selectRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (selectRef.current != null) {
      selectRef.current.value = (args.shipDesignName && args.shipDesignName) || "";
    }
  }, [args.shipDesignName]);

  function handleDesignListSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;
    args.setShipDesignName(value);
  }

  function shipDesignDetails(render: {content: string | null; activeAnchor: HTMLElement | null}) {
    if (render.content == null) {
      return <></>;
    }
    const design = args.shipDesigns[render.content];
    if (design == null) {
      return <>Select a ship design.</>;
    }

    const compressed = Object.values(compressedWeaponsFromTemplate(design));
    const describeWeapon = (weapon: {kind: string; mount: WeaponMount; total: number}) => {
      const weapon_name = weaponToString(createWeapon(weapon.kind, weapon.mount));

      const [quant, suffix] = weapon.total === 1 ? ["a", ""] : [weapon.total, "s"];
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
      weaponDesc.push("and " + describeWeapon(compressed[compressed.length - 1]));
      weaponDesc = ["Weapons are "].concat(weaponDesc);
    }
    return (
      <>
        <h3>{design.name}</h3>
        <div className="ship-design-description-tooltip">
          {design.displacement} tons with {design.hull} hull points and {design.armor} armor.&nbsp;
          {design.power} power back {design.maneuver}G thrust and jump {design.jump}. {weaponDesc}.
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
                value={design.name}>{`${design.name} (${design.displacement})`}</option>
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
