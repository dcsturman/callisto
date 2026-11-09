import * as React from "react";
import { useMemo, useState } from "react";
import { Accordion } from "lib/Accordion";
import { Ship } from "lib/entities";
import { MAX_SQUADRON, sizeAfterMove, teamSquadrons } from "lib/squadrons";
import { renameSquadron, setSquadron } from "lib/serverManager";
import { TEAMS, TEAM_CSS } from "lib/teams";
import { useAppDispatch, useAppSelector } from "state/hooks";
import { setFleetSelection, tickShips } from "state/uiSlice";
import { entitiesSelector } from "state/serverSlice";

/** The chooser's value for "no team". */
const UNALIGNED = "";
/** The move target's value for taking ships out of their squadron. */
const NONE = "\u0000none";
/** The move target's value for a squadron not yet named. */
const NEW = "\u0000new";

/** One group the panel lists: a squadron, or the ships in none. */
interface Group {
  name: string | null;
  ships: Ship[];
}

/**
 * A team's squadrons from the top down: every ship of the team, grouped by
 * squadron, to tick and move in one go, and each squadron to rename or
 * disband. For the referee, any team; for a fleet commander, their own.
 */
export function SquadronsPanel(args: { onlyTeam?: string | null }) {
  const entities = useAppSelector(entitiesSelector);
  const teams = useMemo(() => {
    const present = TEAMS.filter((team) => entities.ships.some((ship) => ship.team === team)) as string[];
    const unaligned = entities.ships.some((ship) => ship.team == null);
    return unaligned ? [...present, UNALIGNED] : present;
  }, [entities.ships]);
  const dispatch = useAppDispatch();
  // The ticks are shared with the roster and the 3D view's box-select.
  const selected = useAppSelector((state) => state.ui.fleetSelection);
  const setSelected = (names: string[]) => dispatch(setFleetSelection(names));
  // Ships ticked elsewhere bring their team up with them, so a referee who
  // boxes Red ships sees Red here; picking a team clears the ticks.
  const selectionTeam = useMemo(() => {
    const picked = entities.ships.filter((ship) => selected.includes(ship.name));
    const sides = new Set(picked.map((ship) => ship.team ?? UNALIGNED));
    return sides.size === 1 ? [...sides][0] : null;
  }, [entities.ships, selected]);
  const [chosenTeam, setChosenTeam] = useState<string | null>(null);
  const team =
    args.onlyTeam ??
    selectionTeam ??
    (chosenTeam != null && teams.includes(chosenTeam) ? chosenTeam : teams[0] ?? UNALIGNED);
  const teamOrNull = team === UNALIGNED ? null : team;

  const [chosenTarget, setTarget] = useState<string>("");
  const [newName, setNewName] = useState("");
  const [renaming, setRenaming] = useState<{ from: string; text: string } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  const groups: Group[] = useMemo(() => {
    const own = entities.ships.filter((ship) => (ship.team ?? null) === teamOrNull);
    const byName = (name: string) => own.filter((ship) => ship.squadron === name);
    const named = teamSquadrons(entities.ships, teamOrNull).map((found) => ({
      name: found.name,
      ships: byName(found.name).sort((a, b) => a.name.localeCompare(b.name)),
    }));
    const loose = own.filter((ship) => ship.squadron == null).sort((a, b) => a.name.localeCompare(b.name));
    return [...named, ...(loose.length > 0 ? [{ name: null, ships: loose }] : [])];
  }, [entities.ships, teamOrNull]);

  // Only ships still on this team stay ticked.
  const onTeam = new Set(groups.flatMap((group) => group.ships.map((ship) => ship.name)));
  const ticked = selected.filter((name) => onTeam.has(name));

  // Where the Move button sends them: what was chosen, while it still
  // exists, else the first squadron, else a new one.
  const squadronNames = groups.flatMap((group) => (group.name == null ? [] : [group.name]));
  const target = [NEW, NONE, ...squadronNames].includes(chosenTarget) ? chosenTarget : squadronNames[0] ?? NEW;

  const toggle = (names: string[], on: boolean) => dispatch(tickShips({ ships: names, on }));

  const move = () => {
    setProblem(null);
    if (ticked.length === 0) {
      return;
    }
    if (target === NONE) {
      setSquadron(ticked, null);
    } else {
      const name = target === NEW ? newName.trim() : target;
      if (name === "") {
        setProblem("Name the new squadron first.");
        return;
      }
      const size = sizeAfterMove(entities.ships, teamOrNull, name, ticked);
      if (size > MAX_SQUADRON) {
        setProblem(`A squadron holds at most ${MAX_SQUADRON} ships; ${name} would have ${size}.`);
        return;
      }
      setSquadron(ticked, name);
      setNewName("");
      setTarget(name);
    }
    setSelected([]);
  };

  return (
    <Accordion
      id="squadrons"
      // The count says, folded, that there is something inside.
      title={`Squadrons (${groups.filter((group) => group.name != null).length})${
        ticked.length > 0 ? ` · ${ticked.length} ticked` : ""
      }`}
      initialOpen={false}
      className="squadrons-panel"
    >
      {args.onlyTeam == null && teams.length > 1 && (
        <select
          className="team-select squadrons-team"
          aria-label="Team"
          value={team}
          style={{ color: teamOrNull ? TEAM_CSS[teamOrNull as keyof typeof TEAM_CSS] : undefined }}
          onChange={(e) => {
            setChosenTeam(e.target.value);
            setSelected([]);
          }}
        >
          {teams.map((name) => (
            <option key={name} value={name}>
              {name === UNALIGNED ? "No team" : name}
            </option>
          ))}
        </select>
      )}
      {groups.length === 0 && <p className="squadrons-empty">This team has no ships.</p>}
      {groups.map((group) => {
        const names = group.ships.map((ship) => ship.name);
        const all = names.every((name) => ticked.includes(name));
        return (
          <div key={group.name ?? "\u0000loose"} className="squadrons-group">
            <div className="squadrons-group-header">
              <input
                type="checkbox"
                aria-label={`Select every ship in ${group.name ?? "no squadron"}`}
                checked={all}
                onChange={(e) => toggle(names, e.target.checked)}
              />
              {renaming != null && renaming.from === group.name ? (
                <>
                  <input
                    className="entity-list-name-input"
                    aria-label="Squadron's new name"
                    autoFocus
                    maxLength={40}
                    value={renaming.text}
                    onChange={(e) => setRenaming({ ...renaming, text: e.target.value })}
                    onKeyDown={(e) => {
                      if (e.key === "Enter" && renaming.text.trim() !== "") {
                        renameSquadron(teamOrNull, renaming.from, renaming.text.trim());
                        setRenaming(null);
                      } else if (e.key === "Escape") {
                        setRenaming(null);
                      }
                    }}
                  />
                </>
              ) : (
                <span className="squadrons-group-name">
                  {group.name ?? "No squadron"}{" "}
                  <span className="squadrons-count">
                    {group.name == null ? `(${names.length})` : `(${names.length}/${MAX_SQUADRON})`}
                  </span>
                </span>
              )}
              {group.name != null && renaming == null && (
                <>
                  <button
                    type="button"
                    className="squadrons-link"
                    onClick={() => setRenaming({ from: group.name as string, text: group.name as string })}
                  >
                    Rename
                  </button>
                  <button type="button" className="squadrons-link" onClick={() => setSquadron(names, null)}>
                    Disband
                  </button>
                </>
              )}
            </div>
            <ul className="squadrons-ships">
              {group.ships.map((ship) => (
                <li key={ship.name}>
                  <label>
                    <input
                      type="checkbox"
                      checked={ticked.includes(ship.name)}
                      onChange={(e) => toggle([ship.name], e.target.checked)}
                    />
                    <span className="squadrons-ship-name">{ship.name}</span>
                    <span className="squadrons-design">{ship.design}</span>
                  </label>
                </li>
              ))}
            </ul>
          </div>
        );
      })}
      {groups.length > 0 && (
        <div className="squadrons-move">
          <span>Move {ticked.length} to</span>
          <select
            className="team-select"
            aria-label="Move the selected ships to"
            value={target}
            onChange={(e) => setTarget(e.target.value)}
          >
            {groups
              .filter((group) => group.name != null)
              .map((group) => (
                <option
                  key={group.name}
                  value={group.name as string}
                  disabled={sizeAfterMove(entities.ships, teamOrNull, group.name as string, ticked) > MAX_SQUADRON}
                >
                  {group.name} ({group.ships.length}/{MAX_SQUADRON})
                </option>
              ))}
            <option value={NEW}>New squadron{"…"}</option>
            <option value={NONE}>No squadron</option>
          </select>
          {target === NEW && (
            <input
              className="entity-list-name-input"
              aria-label="New squadron's name"
              maxLength={40}
              placeholder="Name"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && move()}
            />
          )}
          <button type="button" className="blue-button squadrons-move-button" disabled={ticked.length === 0} onClick={move}>
            Move
          </button>
        </div>
      )}
      {problem != null && <p className="squadrons-problem">{problem}</p>}
    </Accordion>
  );
}
