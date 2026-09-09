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
import {
  Gun,
  Weapon,
  WeaponMount,
  createWeapon,
  weaponToString,
  mountToString,
  WEAPON_MODIFIERS,
} from "lib/weapon";
import {
  DEFAULT_GUNNERY,
  MOUNT_OPTIONS,
  weaponKindsForMount,
  weaponKindLabel,
  isLegalPairing,
  gunCapacity,
  describeGroupGuns,
  legalMountsLabel,
  needsDetailEditor,
  WEAPON_KINDS,
  WeaponGroup,
  checkAllowance,
  commonGunnery,
  emptyGroup,
  expandGroups,
  groupWeapons,
  mountForOptionId,
  mountOptionId,
  mountOptionsFor,
  setAllGunnery,
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

  // One editor row per *run* of identical weapons rather than one per
  // hardpoint, so a ship with thirty matching turrets is one row and not
  // thirty.  A trailing empty row is always present as the next slot to fill.
  const buildWeaponRows = useCallback(
    (designName: string, existing?: Weapon[], gunnery?: number[]) => {
      const design = shipDesignTemplates[designName];
      if (!design) {
        return [];
      }
      const groups = groupWeapons(existing ?? design.weapons, gunnery ?? []);
      return [...groups, emptyGroup(commonGunnery(groups) ?? DEFAULT_GUNNERY)];
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
      crew: createCrew(),
      armament: buildWeaponRows(firstDesign.name),
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
        // An existing ship's own armament, which may differ from its design's,
        // and the gunner skill already recorded against each of its weapons.
        armament: buildWeaponRows(
          current.design,
          shipWeapons(current, shipDesignTemplates),
          current.crew?.gunnery,
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
              armament: buildWeaponRows(
                ship.design,
                shipWeapons(ship, shipDesignTemplates),
                ship.crew?.gunnery,
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

      const ship = findShip(entities, name) || defaultShip();

      // Groups flatten to a dense list here: `weapon_id` stays a 0-based index
      // into the ship's weapons, exactly as FireAction and BoostTarget assume,
      // and `gunnery` is emitted index-aligned with it so weapon N and the
      // skill firing it always carry the same id.  Rows are a UI concept and
      // never go over the wire.
      const { weapons, gunnery } = expandGroups(addShipData.armament);
      const crew = { ...addShipData.crew, gunnery };
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
        armament: buildWeaponRows(design),
      }),
    [addShipData, setAddShipData, buildWeaponRows],
  );

  const handleWeaponsChange = useCallback(
    (armament: WeaponGroup[]) => setAddShipData({ ...addShipData, armament }),
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
          groups={addShipData.armament}
          setGroups={handleWeaponsChange}
        />
        <hr />
        <CrewBuilder
          shipName={addShipData.name}
          currentCrew={addShipData.crew}
          updateCrew={handleCrewChange}
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

// One row per run of identical weapons — count, mount, weapon and the gunner
// skill serving them — with a running total against what the hull allows.  The
// allowance is advisory: the engine never validates armament, so an
// over-allowance ship is flagged but still submittable.
function HardpointList(args: {
  design: ShipDesignTemplate | undefined;
  groups: WeaponGroup[];
  setGroups: (groups: WeaponGroup[]) => void;
}) {
  const displacement = args.design?.displacement ?? 0;

  const report = useMemo(
    () => checkAllowance(args.groups, displacement),
    [args.groups, displacement],
  );

  const bulkGunnery = useMemo(() => commonGunnery(args.groups), [args.groups]);

  // Which row is open in the detail editor, if any.
  const [detailRow, setDetailRow] = useState<number | null>(null);

  // Small craft cannot carry a double or triple turret, or a bay at all, so
  // those are not offered on a firmpoint hull.
  const options = useMemo(
    () => mountOptionsFor(report.allowance.kind),
    [report.allowance.kind],
  );

  const replaceRow = useCallback(
    (index: number, group: WeaponGroup) => {
      const next = args.groups.slice();
      next[index] = group;
      // Filling the last row opens a fresh one beneath it, so there is always
      // somewhere to add the next mount without hunting for a button.
      if (group.mount !== null && index === next.length - 1) {
        next.push(emptyGroup(group.gunnery));
      }
      args.setGroups(next);
    },
    [args],
  );

  const handleMountChange = useCallback(
    (index: number, optionId: string) => {
      const mount = mountForOptionId(optionId);
      // Clearing a row removes it outright rather than leaving a hole; the
      // trailing empty row is the only empty one the editor keeps.
      if (mount === null && index < args.groups.length - 1) {
        args.setGroups(args.groups.filter((_, i) => i !== index));
        return;
      }
      // Not every weapon fits every mount, so switching to a bay while holding
      // a sandcaster has to move the weapon too.  Falling back to the first
      // legal option keeps the row valid instead of leaving it unfireable.
      const group = args.groups[index];
      const kind = isLegalPairing(group.kind, mount)
        ? group.kind
        : (weaponKindsForMount(mount)[0] ?? group.kind);
      // A mount holds a fixed number of guns, so changing it resizes the list.
      // Dropping to a single-gun mount makes the row uniform again.
      const size = gunCapacity(mount);
      const guns =
        group.guns == null || size < 2
          ? undefined
          : Array.from({ length: size }, (_unused, n) => group.guns![n] ?? { kind });
      replaceRow(index, { ...group, mount, kind, guns });
    },
    [args, replaceRow],
  );

  const handleKindChange = useCallback(
    (index: number, kind: string) => {
      const group = args.groups[index];
      if (kind === CUSTOMIZE_OPTION) {
        setDetailRow(index);
        return;
      }
      // Choosing a single weapon makes the mount uniform again.  Clearing the
      // gun list matters: leaving it would show the new kind while still
      // writing the old guns.
      replaceRow(index, { ...group, kind, guns: undefined });
    },
    [args.groups, replaceRow],
  );


  // Zero is allowed so the field can be cleared mid-edit; a zero-count row
  // simply contributes no weapons.
  const handleCountChange = useCallback(
    (index: number, value: string) =>
      replaceRow(index, {
        ...args.groups[index],
        count: Math.max(0, Math.floor(Number(value) || 0)),
      }),
    [args.groups, replaceRow],
  );

  const handleGunneryChange = useCallback(
    (index: number, value: string) =>
      replaceRow(index, {
        ...args.groups[index],
        gunnery: Math.max(0, Math.floor(Number(value) || 0)),
      }),
    [args.groups, replaceRow],
  );

  const handleBulkGunneryChange = useCallback(
    (value: string) =>
      args.setGroups(
        setAllGunnery(args.groups, Math.max(0, Math.floor(Number(value) || 0))),
      ),
    [args],
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
      <label className="hardpoint-bulk-gunnery">
        Gunner skill
        <input
          className="control-input hardpoint-gunnery"
          name="hardpoint-bulk-gunnery"
          type="number"
          min={0}
          /* Blank once any row is overridden, rather than implying agreement
             the armament does not have. */
          value={bulkGunnery ?? ""}
          placeholder="mixed"
          aria-label="Gunner skill for every weapon"
          onChange={(event) => handleBulkGunneryChange(event.target.value)}
        />
      </label>
      <div className="hardpoint-row hardpoint-column-labels">
        <span>Qty</span>
        <span>Mount</span>
        <span>Weapon</span>
        <span>Skill</span>
      </div>
      {args.groups.map((group, index) => {
        const optionId = mountOptionId(group.mount);
        const problem = report.rowProblems[index];
        const empty = group.mount === null;
        const rowOptions =
          optionId != null && !options.some((option) => option.id === optionId)
            ? [...options, ...MOUNT_OPTIONS.filter((o) => o.id === optionId)]
            : options;
        return (
          <div
            className="hardpoint-row"
            key={"hardpoint-" + index}
            title={problem ?? undefined}
          >
            <input
              className={
                problem
                  ? "control-input hardpoint-count hardpoint-over"
                  : "control-input hardpoint-count"
              }
              name={"hardpoint-count-" + index}
              type="number"
              min={0}
              aria-label={"Number of mounts in group " + (index + 1)}
              value={empty ? "" : group.count}
              disabled={empty}
              onChange={(event) => handleCountChange(index, event.target.value)}
            />
            <select
              className={
                empty
                  ? "select-dropdown control-input hardpoint-mount hardpoint-mount-wide"
                  : "select-dropdown control-input hardpoint-mount"
              }
              name={"hardpoint-mount-" + index}
              aria-label={"Group " + (index + 1) + " mount"}
              value={optionId ?? "unsupported"}
              onChange={(event) => handleMountChange(index, event.target.value)}
            >
              {/* A mount no option covers (a mixed turret stored oddly, say)
                  still has to be visible rather than silently rewritten. */}
              {optionId === null && group.mount != null && (
                <option value="unsupported">
                  {weaponToString({ kind: group.kind, mount: group.mount })}
                </option>
              )}
              {/* A design may already carry a mount this hull may not choose —
                  an out-of-allowance turret, say.  Keep it listed so the row
                  shows what the ship really has instead of blanking. */}
              {rowOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
            {!empty && (
              <select
                className="select-dropdown control-input hardpoint-weapon"
                name={"hardpoint-weapon-" + index}
                aria-label={"Group " + (index + 1) + " weapon"}
                value={needsDetailEditor(group) ? CUSTOMIZE_OPTION : group.kind}
                onChange={(event) => handleKindChange(index, event.target.value)}
              >
                {/* A design may name a weapon kind this build does not list.
                    Keep it selectable so existing data is never silently
                    rewritten. */}
                {!needsDetailEditor(group) && !WEAPON_KINDS.includes(group.kind) && (
                  <option value={group.kind}>{weaponKindLabel(group.kind)}</option>
                )}
                {/* Every weapon is listed, with the ones this mount cannot hold
                    greyed out rather than hidden.  Omitting them made a missing
                    weapon look like a broken list instead of a rule -- there is
                    no ion turret, and that is worth showing rather than hiding. */}
                {WEAPON_KINDS.map((kind) => {
                  const legal = isLegalPairing(kind, group.mount);
                  return (
                    <option
                      key={kind}
                      value={kind}
                      disabled={!legal}
                      title={
                        legal
                          ? undefined
                          : `${weaponKindLabel(kind)} needs ${legalMountsLabel(kind)}`
                      }
                    >
                      {legal
                        ? weaponKindLabel(kind)
                        : `${weaponKindLabel(kind)} — needs ${legalMountsLabel(kind)}`}
                    </option>
                  );
                })}
                {/* Anything the four columns cannot express -- a turret of
                    different weapons, or any Advantage fitted to a gun -- opens
                    the detail editor.  When the row already is one of those,
                    this entry is what the cell shows, naming the real contents
                    rather than pretending a single weapon is selected. */}
                <option value={CUSTOMIZE_OPTION}>
                  {needsDetailEditor(group)
                    ? describeGroupGuns(group)
                    : "Customize\u2026"}
                </option>
              </select>
            )}
            {!empty && (
              <input
                className="control-input hardpoint-gunnery"
                name={"hardpoint-gunnery-" + index}
                type="number"
                min={0}
                aria-label={"Group " + (index + 1) + " gunner skill"}
                value={group.gunnery}
                onChange={(event) =>
                  handleGunneryChange(index, event.target.value)
                }
              />
            )}
          </div>
        );
      })}
      {report.problems.map((problem) => (
        <div className="hardpoint-problem" key={problem}>
          {problem}
        </div>
      ))}
      {detailRow != null && args.groups[detailRow] != null && (
        <WeaponDetailDialog
          group={args.groups[detailRow]}
          index={detailRow}
          onChange={(group) => replaceRow(detailRow, group)}
          onClose={() => setDetailRow(null)}
        />
      )}
    </div>
  );
}

/**
 * Detail editor for one hardpoint row.
 *
 * The inline row handles the common case -- a mount of identical, unmodified
 * weapons -- in four columns.  Anything past that (a turret holding different
 * weapons, or any Advantage fitted to a gun) needs more room than a side panel
 * has, so it moves here.  Both kind and modifiers are per-gun properties, which
 * is why this lists guns rather than hanging anything off the mount.
 */
const WeaponDetailDialog = (props: {
  group: WeaponGroup;
  index: number;
  onChange: (group: WeaponGroup) => void;
  onClose: () => void;
}) => {
  const { group, onChange, onClose } = props;
  const mount = group.mount;
  const size = gunCapacity(mount);

  // Editing always works on an explicit gun list, even for a uniform mount, so
  // the dialog has one shape.  It collapses back on close if nothing differs.
  const guns: Gun[] =
    group.guns ??
    Array.from({ length: size }, () => ({
      kind: group.kind,
      modifiers: group.modifiers,
    }));

  const commit = (next: Gun[]) => {
    const uniformKind = next.every((gun) => gun.kind === next[0].kind);
    const sameMods = next.every(
      (gun) =>
        JSON.stringify(gun.modifiers ?? []) ===
        JSON.stringify(next[0].modifiers ?? []),
    );
    // A mount whose guns all match is an ordinary uniform one again, and is
    // stored that way so the row and the wire stay simple.
    if (uniformKind && sameMods) {
      onChange({
        ...group,
        kind: next[0].kind,
        modifiers: next[0].modifiers ?? [],
        guns: undefined,
      });
    } else {
      onChange({ ...group, kind: next[0].kind, guns: next });
    }
  };

  const setGunKind = (gunIndex: number, kind: string) =>
    commit(guns.map((gun, n) => (n === gunIndex ? { ...gun, kind } : gun)));

  const toggleModifier = (gunIndex: number, modifier: string) =>
    commit(
      guns.map((gun, n) => {
        if (n !== gunIndex) {
          return gun;
        }
        const current = gun.modifiers ?? [];
        return {
          ...gun,
          modifiers: current.includes(modifier)
            ? current.filter((m) => m !== modifier)
            : [...current, modifier],
        };
      }),
    );

  return (
    <div className="weapon-detail-backdrop" onClick={onClose}>
      <div
        className="weapon-detail-dialog"
        onClick={(event) => event.stopPropagation()}
      >
        <h2>{mount == null ? "Weapon" : mountToString(mount)}</h2>
        <div className="weapon-detail-summary">
          {group.count} x {describeGroupGuns({ ...group, guns })}
        </div>

        {guns.map((gun, gunIndex) => (
          <div className="weapon-detail-gun" key={"detail-gun-" + gunIndex}>
            <div className="weapon-detail-gun-head">
              {size > 1 && (
                <span className="weapon-detail-gun-label">
                  Gun {gunIndex + 1}
                </span>
              )}
              <select
                className="select-dropdown control-input"
                aria-label={"Gun " + (gunIndex + 1) + " weapon"}
                value={gun.kind}
                onChange={(event) => setGunKind(gunIndex, event.target.value)}
              >
                {!WEAPON_KINDS.includes(gun.kind) && (
                  <option value={gun.kind}>{weaponKindLabel(gun.kind)}</option>
                )}
                {WEAPON_KINDS.map((kind) => {
                  const legal = isLegalPairing(kind, mount);
                  return (
                    <option key={kind} value={kind} disabled={!legal}>
                      {legal
                        ? weaponKindLabel(kind)
                        : `${weaponKindLabel(kind)} — needs ${legalMountsLabel(kind)}`}
                    </option>
                  );
                })}
              </select>
            </div>
            <div className="weapon-detail-modifiers">
              {WEAPON_MODIFIERS.map((modifier) => (
                <label
                  className={
                    "weapon-detail-modifier" +
                    (modifier.inert ? " weapon-detail-modifier-inert" : "")
                  }
                  key={"mod-" + gunIndex + "-" + modifier.kind}
                >
                  <input
                    type="checkbox"
                    checked={(gun.modifiers ?? []).includes(modifier.kind)}
                    onChange={() => toggleModifier(gunIndex, modifier.kind)}
                  />
                  {modifier.label}
                </label>
              ))}
            </div>
          </div>
        ))}

        <button
          className="control-input control-button blue-button"
          onClick={onClose}
        >
          Done
        </button>
      </div>
    </div>
  );
};

/** Sentinel value for the weapon dropdown's entry that opens the detail editor. */
const CUSTOMIZE_OPTION = "__customize__";

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
    () => (weapon: { kind: string; mount: WeaponMount; total: number; guns?: Gun[] }) => {
      const weapon_name = weaponToString(
        weapon.guns != null
          ? { mount: weapon.mount, guns: weapon.guns }
          : createWeapon(weapon.kind, weapon.mount),
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
      {/* Source and Design share one grid so their dropdowns line up on the
          same left edge; a per-row flex would size each label to its own text
          and stagger them. */}
      <div className="design-picker">
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
        <div className="control-label label-with-tooltip">
          Design
          {ciCircle}
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
      </div>
      <Tooltip
        id={args.shipDesignName + "ship-description-tip"}
        className="tooltip-body"
        render={ShipDesignDetails}
      />
      <Tooltip
        id="design-tooltip"
        anchorSelect=".info-icon"
        content="Select the design of the ship you wish to add to the scenario."
      />
    </>
  );
}
