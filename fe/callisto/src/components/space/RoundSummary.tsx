import * as React from "react";
import { ShipRef } from "components/controls/ShipNameTag";
import { Team, teamLabelColor } from "lib/teams";

/** A ship lost this round. */
export interface Loss {
  ship: ShipRef;
  /** Whoever took the most hull off her this round; for a craft lost aboard, whoever destroyed her carrier. */
  by?: ShipRef | null;
  /** The carrier she was lost in, for a craft lost in her berth. */
  aboard?: string | null;
  squadron?: string | null;
  tons: number;
}

/** A ship hurt this round and still in the fight. */
export interface Damaged {
  ship: ShipRef;
  hull_lost: number;
  hull: number;
  max_hull: number;
  criticals: number;
  /** Who hit her, most hull first. */
  by: ShipRef[];
  squadron?: string | null;
  tons: number;
}

/** One side's fire in a round. */
export interface SideFire {
  team?: Team | null;
  attacks: number;
  hits: number;
  hull: number;
  criticals: number;
  /** The side's missiles stopped by point defence. */
  intercepted: number;
  kills: number;
}

/** A round's results added up, as the server sends them. */
export interface RoundSummary {
  losses?: Loss[];
  damaged?: Damaged[];
  fire?: SideFire[];
}

/** Below this a ship is a small craft, and is rolled up with her squadron. */
const SMALL_CRAFT_TONS = 100;

/** The most hurt ships listed before the rest are left to the details. */
const MAX_DAMAGED_ROWS = 40;

const count = (n: number) => n.toLocaleString();

/**
 * Ships in the order given, small craft of one squadron gathered into one
 * group where the first of them stood. Anything else is a group of one.
 */
function rollUp<T extends { ship: ShipRef; squadron?: string | null; tons: number }>(items: T[]): T[][] {
  const groups: T[][] = [];
  const bySquadron = new Map<string, T[]>();
  for (const item of items) {
    if (item.tons < SMALL_CRAFT_TONS && item.squadron) {
      const key = `${item.ship.team ?? ""}/${item.squadron}`;
      const group = bySquadron.get(key);
      if (group) {
        group.push(item);
        continue;
      }
      const fresh = [item];
      bySquadron.set(key, fresh);
      groups.push(fresh);
    } else {
      groups.push([item]);
    }
  }
  return groups;
}

/** "Intrepid ×4, Dauntless": who did it, most often first. */
function tally(ships: (ShipRef | null | undefined)[]): { ship: ShipRef; times: number }[] {
  const seen = new Map<string, { ship: ShipRef; times: number }>();
  for (const ship of ships) {
    if (ship == null) continue;
    const entry = seen.get(ship.name);
    if (entry) {
      entry.times += 1;
    } else {
      seen.set(ship.name, { ship, times: 1 });
    }
  }
  return [...seen.values()].sort((a, b) => b.times - a.times || a.ship.name.localeCompare(b.ship.name));
}

/** A ship's name that opens her lines in the details. */
function ShipLink(args: { ship: ShipRef; onShip: (name: string) => void }) {
  return (
    <button
      type="button"
      className="result-ship summary-ship"
      style={{ color: teamLabelColor(args.ship.team ?? null) }}
      title={`Show ${args.ship.name}'s results`}
      onClick={() => args.onShip(args.ship.name)}>
      {args.ship.name}
    </button>
  );
}

function ByList(args: { by: { ship: ShipRef; times: number }[]; onShip: (name: string) => void; most?: number }) {
  const most = args.most ?? 3;
  const shown = args.by.slice(0, most);
  return (
    <span className="summary-by">
      {"by "}
      {shown.map(({ ship, times }, index) => (
        <React.Fragment key={ship.name}>
          {index > 0 && ", "}
          <ShipLink ship={ship} onShip={args.onShip} />
          {times > 1 && <span className="summary-times"> ×{times}</span>}
        </React.Fragment>
      ))}
      {args.by.length > most && <span className="summary-times"> and {args.by.length - most} more</span>}
    </span>
  );
}

/** The craft in a rolled-up group, small, each a link. */
function Members(args: { ships: ShipRef[]; onShip: (name: string) => void }) {
  return (
    <span className="summary-members">
      {args.ships.map((ship, index) => (
        <React.Fragment key={ship.name}>
          {index > 0 && " "}
          <ShipLink ship={ship} onShip={args.onShip} />
        </React.Fragment>
      ))}
    </span>
  );
}

/** Hull left, with what this round took drawn in the warning colour beside it. */
function HullBar(args: { hull: number; lost: number; max: number }) {
  const max = Math.max(args.max, 1);
  const left = Math.min(100, (args.hull / max) * 100);
  const lost = Math.min(100 - left, (args.lost / max) * 100);
  return (
    <span
      className="summary-hull"
      role="img"
      aria-label={`${count(args.hull)} of ${count(args.max)} hull left, ${count(args.lost)} lost this round`}>
      <span className="summary-hull-left" style={{ width: `${left}%` }} />
      <span className="summary-hull-lost" style={{ width: `${lost}%` }} />
    </span>
  );
}

function Fire(args: { fire: SideFire[] }) {
  if (args.fire.length === 0) return null;
  return (
    <div className="summary-table-wrap">
      <table className="summary-fire">
        <thead>
          <tr>
            <th scope="col">Side</th>
            <th scope="col">Attacks</th>
            <th scope="col">Hits</th>
            <th scope="col">Hull</th>
            <th scope="col">Crits</th>
            <th scope="col">Kills</th>
            <th scope="col" title="The side's missiles that enemy point defence stopped">Shot down</th>
          </tr>
        </thead>
        <tbody>
          {args.fire.map((side) => (
            <tr key={side.team ?? "none"}>
              <th scope="row" style={{ color: teamLabelColor(side.team ?? null) }}>
                {side.team ?? "No side"}
              </th>
              <td>{count(side.attacks)}</td>
              <td>
                {count(side.hits)}
                {side.attacks > 0 && (
                  <span className="summary-times"> {Math.round((side.hits / side.attacks) * 100)}%</span>
                )}
              </td>
              <td>{count(side.hull)}</td>
              <td>{count(side.criticals)}</td>
              <td>{count(side.kills)}</td>
              <td>{count(side.intercepted)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Losses(args: { losses: Loss[]; onShip: (name: string) => void }) {
  // Side by side, each side's losses together and the big ships first.
  const sides = new Map<string, Loss[]>();
  for (const loss of [...args.losses].sort((a, b) => b.tons - a.tons || a.ship.name.localeCompare(b.ship.name))) {
    const key = loss.ship.team ?? "";
    sides.set(key, [...(sides.get(key) ?? []), loss]);
  }
  return (
    <>
      {[...sides.entries()].map(([team, losses]) => (
        <div key={team} className="summary-side">
          <h3 style={{ color: teamLabelColor((team || null) as Team | null) }}>
            {team || "No side"} lost {losses.length}
          </h3>
          {rollUp(losses).map((group) => {
            const first = group[0];
            if (group.length === 1) {
              return (
                <p key={first.ship.name} className="summary-row">
                  <ShipLink ship={first.ship} onShip={args.onShip} />
                  {first.aboard && <span className="summary-times"> in her berth aboard {first.aboard}</span>}
                  {first.by && <> <ByList by={tally([first.by])} onShip={args.onShip} /></>}
                </p>
              );
            }
            return (
              <p key={`${first.squadron}-${first.ship.name}`} className="summary-row">
                <span className="summary-squadron">{first.squadron}</span>
                <span className="summary-times"> · {group.length} craft </span>
                <ByList by={tally(group.map((loss) => loss.by))} onShip={args.onShip} />
                <Members ships={group.map((loss) => loss.ship)} onShip={args.onShip} />
              </p>
            );
          })}
        </div>
      ))}
    </>
  );
}

function Hurt(args: { damaged: Damaged[]; onShip: (name: string) => void }) {
  const groups = rollUp(args.damaged);
  return (
    <>
      {groups.slice(0, MAX_DAMAGED_ROWS).map((group) => {
        const first = group[0];
        const lost = group.reduce((sum, hurt) => sum + hurt.hull_lost, 0);
        const hull = group.reduce((sum, hurt) => sum + hurt.hull, 0);
        const max = group.reduce((sum, hurt) => sum + hurt.max_hull, 0);
        const criticals = group.reduce((sum, hurt) => sum + hurt.criticals, 0);
        const by = tally(group.flatMap((hurt) => hurt.by));
        return (
          <div key={`${first.squadron ?? ""}-${first.ship.name}`} className="summary-hurt">
            <span className="summary-hurt-name">
              {group.length === 1 ? (
                <ShipLink ship={first.ship} onShip={args.onShip} />
              ) : (
                <>
                  <span className="summary-squadron" style={{ color: teamLabelColor(first.ship.team ?? null) }}>
                    {first.squadron}
                  </span>
                  <span className="summary-times"> · {group.length} craft</span>
                </>
              )}
            </span>
            <HullBar hull={hull} lost={lost} max={max} />
            <span className="summary-figures">
              {lost > 0 ? `−${count(lost)}` : "no hull"} · {count(hull)}/{count(max)}
              {criticals > 0 && (
                <span className="summary-crits">
                  {" "}
                  · {criticals} crit{criticals === 1 ? "" : "s"}
                </span>
              )}
            </span>
            <span className="summary-hurt-by">
              {by.length > 0 && <ByList by={by} onShip={args.onShip} />}
              {group.length > 1 && <Members ships={group.map((hurt) => hurt.ship)} onShip={args.onShip} />}
            </span>
          </div>
        );
      })}
      {groups.length > MAX_DAMAGED_ROWS && (
        <p className="sensor-empty">{groups.length - MAX_DAMAGED_ROWS} more hurt. The details have them all.</p>
      )}
    </>
  );
}

/**
 * The round added up: each side's fire, who was lost and to whom, and who is
 * hurt and how badly. Small craft are gathered by squadron. Any ship's name
 * opens her own lines in the details.
 */
export function RoundSummaryView(args: { summary: RoundSummary | null; onShip: (name: string) => void }) {
  const losses = args.summary?.losses ?? [];
  const damaged = args.summary?.damaged ?? [];
  const fire = args.summary?.fire ?? [];
  if (losses.length === 0 && damaged.length === 0 && fire.length === 0) {
    return <p className="sensor-empty">Nobody fired on anybody this round.</p>;
  }
  return (
    <div className="round-summary">
      <Fire fire={fire} />
      {losses.length > 0 && (
        <section>
          <h2 className="summary-heading">Lost</h2>
          <Losses losses={losses} onShip={args.onShip} />
        </section>
      )}
      <section>
        <h2 className="summary-heading">Hit</h2>
        {damaged.length === 0 ? (
          <p className="sensor-empty">No ship still in the fight lost hull or took a critical.</p>
        ) : (
          <Hurt damaged={damaged} onShip={args.onShip} />
        )}
      </section>
    </div>
  );
}
