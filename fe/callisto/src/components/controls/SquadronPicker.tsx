import * as React from "react";
import { useMemo, useState } from "react";
import { Ship } from "lib/entities";
import { MAX_SQUADRON, teamSquadrons } from "lib/squadrons";
import { renameSquadron, setSquadron } from "lib/serverManager";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

const NONE = "";
const NEW = "\u0000new";
const RENAME = "\u0000rename";
const DISBAND = "\u0000disband";

/**
 * Which squadron of her team a ship flies in: join one, start one, rename
 * hers, or disband it. A full squadron is shown but cannot be joined.
 */
export function SquadronPicker(args: { ship: Ship }) {
  const entities = useAppSelector(entitiesSelector);
  const { ship } = args;
  const squadrons = useMemo(() => teamSquadrons(entities.ships, ship.team), [entities.ships, ship.team]);
  const current = squadrons.find((found) => found.name === ship.squadron);
  // Naming a new squadron or renaming hers: the text being typed.
  const [naming, setNaming] = useState<{ renaming: boolean; text: string } | null>(null);

  const apply = () => {
    const text = naming?.text.trim() ?? "";
    if (text !== "") {
      if (naming?.renaming && ship.squadron != null) {
        renameSquadron(ship.team ?? null, ship.squadron, text);
      } else {
        setSquadron([ship.name], text);
      }
    }
    setNaming(null);
  };

  if (naming != null) {
    return (
      <span className="squadron-naming">
        <input
          className="control-name-input control-input"
          aria-label={naming.renaming ? "Squadron's new name" : "New squadron's name"}
          autoFocus
          maxLength={40}
          value={naming.text}
          onChange={(e) => setNaming({ ...naming, text: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              apply();
            } else if (e.key === "Escape") {
              setNaming(null);
            }
          }}
        />
        <button type="button" className="control-input" onClick={apply}>
          OK
        </button>
      </span>
    );
  }

  return (
    <select
      className="team-select"
      aria-label="Squadron"
      title={`The squadron this ship flies in, within her team: the unit orders are given to. At most ${MAX_SQUADRON} ships.`}
      value={ship.squadron ?? NONE}
      onChange={(event) => {
        const value = event.target.value;
        if (value === NEW) {
          setNaming({ renaming: false, text: "" });
        } else if (value === RENAME) {
          setNaming({ renaming: true, text: ship.squadron ?? "" });
        } else if (value === DISBAND) {
          setSquadron(current?.ships ?? [ship.name], null);
        } else {
          setSquadron([ship.name], value === NONE ? null : value);
        }
      }}
    >
      <option value={NONE}>None</option>
      {squadrons.map((found) => (
        <option
          key={found.name}
          value={found.name}
          disabled={found.name !== ship.squadron && found.ships.length >= MAX_SQUADRON}
        >
          {found.name} ({found.ships.length}/{MAX_SQUADRON})
        </option>
      ))}
      <option value={NEW}>New squadron{"…"}</option>
      {current != null && <option value={RENAME}>Rename {current.name}{"…"}</option>}
      {current != null && <option value={DISBAND}>Disband {current.name}</option>}
    </select>
  );
}
