import * as React from "react";
import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { CrewBuilder, Crew, createCrew } from "components/controls/CrewBuilder";
import { POSITION_SCALE } from "lib/universal";
import {
  ShipDesignTemplate,
  ShipDesignTemplates,
  compressedWeapons,
  shipWeapons,
} from "lib/shipDesignTemplates";
import { Weapon, WeaponMount, createWeapon, weaponToString } from "lib/weapon";
import {
  DEFAULT_WEAPON_KIND,
  MOUNT_OPTIONS,
  WEAPON_KINDS,
  checkAllowance,
  compactWeaponRows,
  mountForOptionId,
  mountOptionId,
  padWeaponRows,
  rowCountForDesign,
} from "lib/hardpoints";
import { Accordion } from "lib/Accordion";
import { Tooltip } from "react-tooltip";
import { CiCircleQuestion } from "react-icons/ci";
import { unique_ship_name } from "lib/shipnames";
import { Ship, defaultShip, findShip } from "lib/entities";

import { addShip } from "lib/serverManager";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

type AddShipProps = unknown;

export const AddShip: React.FC<AddShipProps> = () => {
  const entities = useAppSelector(entitiesSelector);
  const shipDesignTemplates = useAppSelector((state) => state.server.templates);

  const shipNames = useMemo(
    () => entities.ships.map((ship: Ship) => ship.name),
    [entities.ships],
  );

  const designRef = useRef<HTMLSelectElement>(null);
  const shipNameRef = useRef<HTMLInputElement>(null);

  // One editor row per weapon mount.  Normally that is one row per point of
  // hardpoint/firmpoint allowance; a design whose own armament already exceeds
  // its allowance gets enough rows to show all of it rather than losing mounts.
  const buildWeaponRows = useCallback(
    (designName: string, existing?: Weapon[]) => {
      const design = shipDesignTemplates[designName];
      if (!design) {
        return [];
      }
      const source = existing ?? design.weapons;
      return padWeaponRows(
        source,
        rowCountForDesign(
          design.displacement,
          Math.max(design.weapons.length, source.length),
        ),
      );
    },
    [shipDesignTemplates],
  );

  const initialTemplate = useMemo(() => {
    const firstDesign = Object.values(shipDesignTemplates)[0];
    return {
      name: unique_ship_name(entities),
      xpos: "0",
      ypos: "0",
      zpos: "0",
      xvel: "0",
      yvel: "0",
      zvel: "0",
      design: firstDesign.name,
      crew: createCrew(firstDesign.weapons.length),
      weapons: buildWeaponRows(firstDesign.name),
    };
  }, [shipDesignTemplates, entities, buildWeaponRows]);

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
        // An existing ship's own armament, which may differ from its design's.
        weapons: buildWeaponRows(
          current.design,
          shipWeapons(current, shipDesignTemplates),
        ),
      };
      setAddShipData(template);
    }
  }, [addShipData.name, entities.ships, buildWeaponRows, shipDesignTemplates]);

  const handleChange = useMemo(
    () => (event: React.ChangeEvent<HTMLInputElement>) => {
      if (designRef.current) {
        designRef.current.style.color = "black";
      }

      event.target.style.color = "black";
      if (event.target.name === "name") {
        if (shipNames.includes(event.target.value)) {
          event.target.style.color = "green";
          const ship = findShip(entities, event.target.value);
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
              weapons: buildWeaponRows(
                ship.design,
                shipWeapons(ship, shipDesignTemplates),
              ),
            });
          }
        }
      }
      setAddShipData({
        ...addShipData,
        [event.target.name]: event.target.value,
      });
    },
    [
      designRef,
      shipNames,
      entities,
      setAddShipData,
      addShipData,
      buildWeaponRows,
      shipDesignTemplates,
    ],
  );

  const handleSubmit = useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
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
      setAddShipData({ ...addShipData, design: design });

      const crew = addShipData.crew;
      const ship = findShip(entities, name) || defaultShip();

      // Dense list, empty rows removed: `weapon_id` stays a 0-based index into
      // the ship's weapons, exactly as FireAction and BoostTarget assume.
      // Hardpoint row numbers are a UI concept and never go over the wire.
      const weapons = compactWeaponRows(addShipData.weapons);
      const designWeapons = shipDesignTemplates[design]?.weapons ?? [];
      // Send nothing when the armament is just the design's, so an unmodified
      // ship keeps inheriting from its design rather than freezing a copy.
      const armament = sameWeapons(weapons, designWeapons) ? undefined : weapons;

      const revision = {
        ...ship,
        name,
        position,
        velocity,
        design,
        crew,
        weapons: armament,
      };

      addShip(revision);
      setAddShipData(initialTemplate);
      shipNameRef.current!.style.color = "black";
    },
    [addShipData, entities, initialTemplate, shipNameRef, shipDesignTemplates],
  );

  // Changing the design changes the allowance and the default armament, so the
  // rows are rebuilt from the new design rather than carried over.
  const handleDesignChange = useCallback(
    (design: string) =>
      setAddShipData({
        ...addShipData,
        design: design,
        weapons: buildWeaponRows(design),
      }),
    [addShipData, setAddShipData, buildWeaponRows],
  );

  const handleWeaponsChange = useCallback(
    (weapons: (Weapon | null)[]) =>
      setAddShipData({ ...addShipData, weapons: weapons }),
    [addShipData, setAddShipData],
  );

  const handleCrewChange = useCallback(
    (crew: Crew) => {
      setAddShipData({ ...addShipData, crew: crew });
    },
    [addShipData, setAddShipData],
  );

  const updateOrAddLabel = useMemo(
    () => (shipNames.includes(addShipData.name) ? "Update" : "Add"),
    [addShipData.name, shipNames],
  );

  return (
    <Accordion id="add-ship-header" title="Add Ship" initialOpen={false}>
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
            setShipDesignName={handleDesignChange}
            shipDesigns={shipDesignTemplates}
          />
        </div>
        <hr />
        <HardpointList
          design={shipDesignTemplates[addShipData.design]}
          weapons={addShipData.weapons}
          setWeapons={handleWeaponsChange}
        />
        <hr />
        <CrewBuilder
          shipName={addShipData.name}
          currentCrew={addShipData.crew}
          updateCrew={handleCrewChange}
          num_gunners={compactWeaponRows(addShipData.weapons).length}
        />
        <input
          className="control-input control-button blue-button"
          type="submit"
          value={updateOrAddLabel}
        />
      </form>
    </Accordion>
  );
};

// Two armaments are the same when they are the same weapons in the same order.
// Used to decide whether a ship needs its own weapon list at all.
const sameWeapons = (a: Weapon[], b: Weapon[]) =>
  a.length === b.length &&
  a.every(
    (weapon, index) =>
      weapon.kind === b[index].kind &&
      weaponToString(weapon) === weaponToString(b[index]),
  );

// One row per weapon mount, with a running count against what the hull allows.
// The allowance is advisory: the engine never validates armament, so an
// over-allowance ship is flagged but still submittable.
function HardpointList(args: {
  design: ShipDesignTemplate | undefined;
  weapons: (Weapon | null)[];
  setWeapons: (weapons: (Weapon | null)[]) => void;
}) {
  const displacement = args.design?.displacement ?? 0;

  const report = useMemo(
    () => checkAllowance(args.weapons, displacement),
    [args.weapons, displacement],
  );

  const replaceRow = useCallback(
    (index: number, weapon: Weapon | null) => {
      const next = args.weapons.slice();
      next[index] = weapon;
      args.setWeapons(next);
    },
    [args],
  );

  const handleMountChange = useCallback(
    (index: number, optionId: string) => {
      const mount = mountForOptionId(optionId);
      if (mount === null) {
        replaceRow(index, null);
        return;
      }
      // Keep whatever weapon the row already carried; only the mount changed.
      const kind = args.weapons[index]?.kind ?? DEFAULT_WEAPON_KIND;
      replaceRow(index, createWeapon(kind, mount));
    },
    [args.weapons, replaceRow],
  );

  const handleKindChange = useCallback(
    (index: number, kind: string) => {
      const current = args.weapons[index];
      if (current == null) {
        return;
      }
      replaceRow(index, createWeapon(kind, current.mount));
    },
    [args.weapons, replaceRow],
  );

  if (!args.design) {
    return <></>;
  }

  const { allowance } = report;
  const allowanceLabel =
    allowance.kind === "hardpoints" ? "Hardpoints" : "Firmpoints";

  return (
    <div className="hardpoint-list">
      <div className="hardpoint-header">
        <h3 className="hardpoint-title">{allowanceLabel}</h3>
        <span
          className={
            report.overAllowance
              ? "hardpoint-usage hardpoint-over"
              : "hardpoint-usage"
          }
        >
          {report.used} of {allowance.total} used
        </span>
      </div>
      {args.weapons.map((weapon, index) => {
        const optionId = mountOptionId(weapon?.mount ?? null);
        const problem = report.rowProblems[index];
        return (
          <div
            className="hardpoint-row"
            key={"hardpoint-" + index}
            title={problem ?? undefined}
          >
            <span
              className={
                problem ? "hardpoint-index hardpoint-over" : "hardpoint-index"
              }
            >
              {index + 1}
            </span>
            <select
              className="select-dropdown control-input hardpoint-mount"
              name={"hardpoint-mount-" + index}
              aria-label={"Hardpoint " + (index + 1) + " mount"}
              value={optionId ?? "unsupported"}
              onChange={(event) => handleMountChange(index, event.target.value)}
            >
              {/* A mount no option covers (a mixed turret stored oddly, say)
                  still has to be visible rather than silently rewritten. */}
              {optionId === null && weapon != null && (
                <option value="unsupported">{weaponToString(weapon)}</option>
              )}
              {MOUNT_OPTIONS.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
            {weapon != null && (
              <select
                className="select-dropdown control-input hardpoint-weapon"
                name={"hardpoint-weapon-" + index}
                aria-label={"Hardpoint " + (index + 1) + " weapon"}
                value={weapon.kind}
                onChange={(event) =>
                  handleKindChange(index, event.target.value)
                }
              >
                {/* A design may name a weapon kind this build does not list. */}
                {!WEAPON_KINDS.includes(weapon.kind) && (
                  <option value={weapon.kind}>{weapon.kind}</option>
                )}
                {WEAPON_KINDS.map((kind) => (
                  <option key={kind} value={kind}>
                    {kind}
                  </option>
                ))}
              </select>
            )}
          </div>
        );
      })}
      {report.problems.map((problem) => (
        <div className="hardpoint-problem" key={problem}>
          {problem}
        </div>
      ))}
    </div>
  );
}

const ShipDesignDetails = (render: {
  content: string | null;
  activeAnchor: HTMLElement | null;
}) => {
  const designs = useAppSelector((state) => state.server.templates);
  const design = useMemo(() => {
    if (!render.content || !designs[render.content]) {
      return null;
    }
    return designs[render.content];
  }, [designs, render.content]);
  const compressed = useMemo(
    () => Object.values(compressedWeapons(design?.weapons ?? null)),
    [design],
  );
  const describeWeapon = useMemo(
    () => (weapon: { kind: string; mount: WeaponMount; total: number }) => {
      const weapon_name = weaponToString(
        createWeapon(weapon.kind, weapon.mount),
      );

      const [quant, suffix] =
        weapon.total === 1 ? ["a", ""] : [weapon.total, "s"];
      return `${quant} ${weapon_name}${suffix}`;
    },
    [],
  );

  const weaponDesc: string[] = useMemo(() => {
    if (compressed.length === 0) {
      return ["This ship is unarmed."];
    } else if (compressed.length === 1) {
      return ["Weapons are ", describeWeapon(compressed[0])];
    } else {
      const preamble = compressed
        .slice(0, -1)
        .map((...[weapon]) => describeWeapon(weapon) + ", ");
      return ["Weapons are "].concat(preamble, [
        "and " + describeWeapon(compressed[compressed.length - 1]),
      ]);
    }
  }, [compressed, describeWeapon]);

  if (render.content == null) {
    return <></>;
  }
  if (design == null) {
    return <>Select a ship design.</>;
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
};

// Both `role` and `source` are optional and free-form on the server side, so
// designs missing either still need a home in the picker.
const UNSPECIFIED = "Other";
const ALL_SOURCES = "All";

const byDisplacementThenName = (
  a: ShipDesignTemplate,
  b: ShipDesignTemplate,
) =>
  a.displacement > b.displacement
    ? 1
    : a.displacement < b.displacement
      ? -1
      : a.name.localeCompare(b.name);

// Alphabetical, with the catch-all bucket pinned last.
const byGroupLabel = (a: string, b: string) =>
  a === UNSPECIFIED ? 1 : b === UNSPECIFIED ? -1 : a.localeCompare(b);

function ShipDesignList(args: {
  shipDesignName: string;
  setShipDesignName: (designName: string) => void;
  shipDesigns: ShipDesignTemplates;
}) {
  const [source, setSource] = useState(ALL_SOURCES);

  const selectRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (selectRef.current != null) {
      selectRef.current.value =
        (args.shipDesignName && args.shipDesignName) || "";
    }
  }, [args.shipDesignName]);

  const handleDesignListSelectChange = useCallback(
    (event: React.ChangeEvent<HTMLSelectElement>) => {
      const value = event.target.value;
      args.setShipDesignName(value);
    },
    [args],
  );

  const handleSourceChange = useCallback(
    (event: React.ChangeEvent<HTMLSelectElement>) =>
      setSource(event.target.value),
    [setSource],
  );

  // Every distinct source in the library, derived from the data itself.
  const sources = useMemo(
    () =>
      Array.from(
        new Set(
          Object.values(args.shipDesigns).map(
            (design) => design.source || UNSPECIFIED,
          ),
        ),
      ).sort(byGroupLabel),
    [args.shipDesigns],
  );

  // Designs matching the source filter, bucketed by role for <optgroup>.
  const groups = useMemo(() => {
    const buckets = Object.values(args.shipDesigns)
      .filter(
        (design) =>
          source === ALL_SOURCES || (design.source || UNSPECIFIED) === source,
      )
      .reduce(
        (accumulator, design) => {
          const role = design.role || UNSPECIFIED;
          accumulator[role] = (accumulator[role] || []).concat(design);
          return accumulator;
        },
        {} as { [role: string]: ShipDesignTemplate[] },
      );

    return Object.keys(buckets)
      .sort(byGroupLabel)
      .map((role) => ({
        role,
        designs: buckets[role].sort(byDisplacementThenName),
      }));
  }, [args.shipDesigns, source]);

  const visible = useMemo(
    () =>
      groups.reduce<ShipDesignTemplate[]>(
        (all, group) => all.concat(group.designs),
        [],
      ),
    [groups],
  );

  // The filter can hide whatever is currently selected; move the selection to
  // the first design still on offer so the form never submits a hidden design.
  useEffect(() => {
    if (
      visible.length > 0 &&
      !visible.some((design) => design.name === args.shipDesignName)
    ) {
      args.setShipDesignName(visible[0].name);
    }
  }, [visible, args]);

  const ciCircle = useMemo(
    () => <CiCircleQuestion className="info-icon" />,
    [],
  );

  return (
    <>
      <div className="control-launch-div">
        <div className="control-label">Source</div>
        <select
          className="select-dropdown control-name-input control-input"
          name="ship_source_choice"
          value={source}
          onChange={handleSourceChange}
        >
          <option key="all-ship_source" value={ALL_SOURCES}>
            {ALL_SOURCES}
          </option>
          {sources.map((name) => (
            <option key={name + "-ship_source"} value={name}>
              {name}
            </option>
          ))}
        </select>
      </div>
      <div className="control-launch-div">
        <div className="control-label">
          <div className="control-label label-with-tooltip">
            Design
            {ciCircle}
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
          data-tooltip-delay-show={700}
        >
          {groups.map((group) => (
            <optgroup key={group.role + "-ship_role"} label={group.role}>
              {group.designs.map((design) => (
                <option
                  key={design.name + "-ship_list"}
                  value={design.name}
                >{`${design.name} (${design.displacement})`}</option>
              ))}
            </optgroup>
          ))}
        </select>
        <Tooltip
          id={args.shipDesignName + "ship-description-tip"}
          className="tooltip-body"
          render={ShipDesignDetails}
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
