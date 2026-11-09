import * as React from "react";
import { useMemo, useState } from "react";
import { Accordion } from "lib/Accordion";
import { Ship } from "lib/entities";
import { moveCameraTo } from "lib/camera";
import { centroid, hullShare, squadronStep, teamSquadrons } from "lib/squadrons";
import { embarkedSquadrons } from "lib/carriers";
import { TEAMS, Team, teamLabelColor } from "lib/teams";
import { useAppDispatch, useAppSelector } from "state/hooks";
import { entitiesSelector, templatesSelector } from "state/serverSlice";
import { setComputerShipName, setFleetSelection, setFocusSquadron, tickShips } from "state/uiSlice";

/** One line of the roster: a squadron, or a team's ships in none. */
interface Row {
  key: string;
  team: string | null;
  name: string | null;
  ships: Ship[];
  /** The carrier these craft are aboard: not on the plot, so not selectable. */
  aboard?: string;
}

/** "74%", or a dash when no design is known. */
const percent = (share: number | null) => (share == null ? "–" : `${Math.round(share * 100)}%`);

/** The rows for one team: its squadrons by name, then its ships in none. */
const rowsFor = (ships: Ship[], team: string | null): Row[] => {
  const own = ships.filter((ship) => (ship.team ?? null) === team);
  const named: Row[] = teamSquadrons(ships, team).map((squadron) => ({
    key: `${team ?? ""}\u0000${squadron.name}`,
    team,
    name: squadron.name,
    ships: own.filter((ship) => ship.squadron === squadron.name).sort((a, b) => a.name.localeCompare(b.name)),
  }));
  const loose = own.filter((ship) => ship.squadron == null).sort((a, b) => a.name.localeCompare(b.name));
  // Wings in a carrier's berths are not entities, so they would otherwise
  // be missing from the roster altogether: a line each, under the carrier.
  const embarked: Row[] = embarkedSquadrons(ships)
    .filter((entry) => entry.team === team)
    .map((entry) => ({
      key: `${team ?? ""}\u0000${entry.name ?? ""}\u0000aboard:${entry.carrier}`,
      team,
      name: entry.name,
      ships: entry.ships,
      aboard: entry.carrier,
    }));
  return [...named, ...(loose.length > 0 ? [{ key: `${team ?? ""}\u0000`, team, name: null, ships: loose }] : []), ...embarked];
};

/**
 * The fleet a line a squadron: how many ships it has, how much hull is left,
 * and what the AI is having it do. Each line folds open to its ships.
 *
 * Select ticks the squadron's ships for the Squadrons panel and points the
 * Orders panel at it; Go puts the camera over it. With a hundred ships a side
 * this is how a commander reads the battle without opening each ship. The
 * referee sees every team, a fleet commander their own.
 */
export function SquadronRoster(args: { onlyTeam?: string | null }) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);
  const focus = useAppSelector((state) => state.ui.focusSquadron);
  const selection = useAppSelector((state) => state.ui.fleetSelection);
  const cameraQuaternion = useAppSelector((state) => state.ui.cameraQuaternion);
  const dispatch = useAppDispatch();
  const [open, setOpen] = useState<string[]>([]);

  const teams: (string | null)[] = useMemo(() => {
    if (args.onlyTeam != null) {
      return [args.onlyTeam];
    }
    // A craft aboard takes her carrier's team when she has none, so no team
    // exists only in the berths.
    const present: (string | null)[] = TEAMS.filter((team) => entities.ships.some((ship) => ship.team === team));
    return entities.ships.some((ship) => ship.team == null) ? [...present, null] : present;
  }, [args.onlyTeam, entities.ships]);

  const byTeam = useMemo(
    () => teams.map((team) => ({ team, rows: rowsFor(entities.ships, team) })),
    [teams, entities.ships],
  );
  const squadronCount = byTeam.reduce((n, { rows }) => n + rows.filter((row) => row.name != null).length, 0);

  const select = (row: Row) => {
    dispatch(setFleetSelection(row.ships.map((ship) => ship.name)));
    dispatch(setFocusSquadron(row.name == null ? null : { team: row.team, name: row.name }));
  };
  const fold = (key: string) => setOpen((was) => (was.includes(key) ? was.filter((k) => k !== key) : [...was, key]));

  return (
    <Accordion id="squadron-roster" title={`Roster (${squadronCount})`} initialOpen={true} className="squadron-roster">
      {byTeam.map(({ team, rows }) => (
        <div key={team ?? "\u0000none"} className="roster-team">
          {teams.length > 1 && (
            <h3 className="roster-team-name" style={{ color: teamLabelColor(team as Team | null) }}>
              {team ?? "No team"}
            </h3>
          )}
          {rows.length === 0 && <p className="squadrons-empty">No ships.</p>}
          <table className="roster-table">
            <tbody>
              {rows.map((row) => {
                const isOpen = open.includes(row.key);
                const focused = row.name != null && focus?.name === row.name && (focus?.team ?? null) === row.team;
                const step = row.aboard == null ? squadronStep(row.ships, row.name) : null;
                const label = `${row.name ?? "No squadron"}${row.aboard == null ? "" : ` (aboard ${row.aboard})`}`;
                const carrier = row.aboard == null ? null : entities.ships.find((ship) => ship.name === row.aboard);
                return (
                  <React.Fragment key={row.key}>
                    <tr className={focused ? "roster-row roster-row-focus" : "roster-row"}>
                      <td className="roster-name">
                        <button
                          type="button"
                          className="roster-fold"
                          aria-expanded={isOpen}
                          aria-label={`${isOpen ? "Fold" : "Show"} the ships of ${label}`}
                          onClick={() => fold(row.key)}
                        >
                          {isOpen ? "▾" : "▸"}
                        </button>{" "}
                        <span style={{ color: teamLabelColor(row.team as Team | null) }}>{label}</span>
                      </td>
                      <td className="roster-count" title="Ships">
                        {row.ships.length}
                      </td>
                      <td className="roster-hull" title="Hull left">
                        {percent(hullShare(row.ships, templates))}
                      </td>
                      <td className="roster-actions">
                        {/* Craft aboard cannot be ticked or ordered until they
                            launch; Go finds their carrier. */}
                        {row.aboard == null && (
                          <button
                            type="button"
                            className="squadrons-link"
                            title="Tick these ships for the Squadrons panel, and give this squadron orders"
                            onClick={() => select(row)}
                          >
                            Select
                          </button>
                        )}
                        <button
                          type="button"
                          className="squadrons-link"
                          title={row.aboard == null ? "Put the camera over them" : `Put the camera over ${row.aboard}`}
                          onClick={() =>
                            moveCameraTo(cameraQuaternion, centroid(carrier == null ? row.ships : [carrier]))
                          }
                        >
                          Go
                        </button>
                      </td>
                    </tr>
                    {step != null && (
                      <tr className="roster-step-row">
                        <td colSpan={4} className="roster-step" title={step}>
                          {step}
                        </td>
                      </tr>
                    )}
                    {isOpen &&
                      row.aboard != null &&
                      row.ships.map((ship) => (
                        <tr key={ship.name} className="roster-ship">
                          <td className="roster-name">{ship.name}</td>
                          <td className="roster-design">aboard</td>
                          <td className="roster-hull">{percent(hullShare([ship], templates))}</td>
                          <td className="roster-design" title={ship.design}>
                            {ship.design}
                          </td>
                        </tr>
                      ))}
                    {isOpen &&
                      row.aboard == null &&
                      row.ships.map((ship) => (
                        <tr key={ship.name} className="roster-ship">
                          <td className="roster-name">
                            <label>
                              <input
                                type="checkbox"
                                aria-label={`Tick ${ship.name}`}
                                checked={selection.includes(ship.name)}
                                onChange={(e) => dispatch(tickShips({ ships: [ship.name], on: e.target.checked }))}
                              />
                              <button
                                type="button"
                                className="squadrons-link roster-ship-name"
                                title={`Open ${ship.name}'s console`}
                                onClick={() => dispatch(setComputerShipName(ship.name))}
                              >
                                {ship.name}
                              </button>
                            </label>
                          </td>
                          <td className="roster-design" colSpan={1} title={ship.design}>
                            {ship.controller === "AI" ? "AI" : ""}
                          </td>
                          <td className="roster-hull">{percent(hullShare([ship], templates))}</td>
                          <td className="roster-design" title={ship.design}>
                            {ship.design}
                          </td>
                        </tr>
                      ))}
                  </React.Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      ))}
    </Accordion>
  );
}
