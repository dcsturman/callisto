import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {isUndetected} from "lib/contacts";
import {formatKm, bandName} from "lib/range";
import {teamLabelColor} from "lib/teams";
import {
  PictureRow,
  Salvo,
  Watcher,
  incomingSalvoes,
  teamPicture,
  watchersOf,
} from "lib/sensorPicture";
import {WeaponGlyph} from "components/controls/WeaponGlyph";
import {shipWeapons} from "lib/shipDesignTemplates";
import {useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";
import {EmissionProfile} from "components/controls/EmissionProfile";

/**
 * The sensor operator's station.
 *
 * Everything here is reading rather than ordering: the actions live above in
 * the sensor chooser. The sensop's real job at the table is telling the rest
 * of the crew what is out there, so this is built to be read aloud -- who is
 * looking at us, what our own side can and cannot see, what is inbound, and
 * what we know about each contact.
 */
export function SensorStation(args: {ship: Ship}) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);
  const ships = entities.ships;

  const watchers = useMemo(() => watchersOf(args.ship, ships), [args.ship, ships]);
  const picture = useMemo(() => teamPicture(args.ship, ships), [args.ship, ships]);
  const salvoes = useMemo(
    () => incomingSalvoes(args.ship, entities.missiles),
    [args.ship, entities.missiles]
  );

  return (
    <div className="sensor-station">
      <EmissionProfile ship={args.ship} />
      <EyesOnUs watchers={watchers} />
      {salvoes.length > 0 && <IncomingSalvoes salvoes={salvoes} />}
      {/* A squadron of one has no picture to manage. */}
      {picture.ours.length > 1 && (
        <TeamPicture ours={picture.ours.map((ship) => ship.name)} rows={picture.rows} />
      )}
      <ContactDetail observer={args.ship} ships={ships} templates={templates} />
    </div>
  );
}

/**
 * Who is watching us, and who is aiming.
 *
 * The eye and the crosshair are the whole point: a contact is bad news and a
 * lock is worse, and the difference should not need reading.
 */
function EyesOnUs(args: {watchers: Watcher[]}) {
  return (
    <div className="sensor-block">
      <div className="section-tag">Eyes on us</div>
      {args.watchers.length === 0 ? (
        <p className="sensor-empty">Nobody has found us.</p>
      ) : (
        <ul className="watcher-list">
          {args.watchers.map((watcher) => (
            <li
              key={watcher.name}
              className={watcher.lock ? "watcher watcher-locked" : "watcher"}
              title={
                watcher.lock
                  ? `${watcher.name} holds a sensor lock: DM+2 on every shot at us`
                  : `${watcher.name} has us on sensors`
              }>
              {watcher.lock ? <CrosshairIcon /> : <EyeIcon />}
              <span className="watcher-name" style={{color: teamLabelColor(watcher.team)}}>
                {watcher.name}
              </span>
              <span className="watcher-state">{watcher.lock ? "LOCKED" : "contact"}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Inbound missiles, by whoever threw them. */
function IncomingSalvoes(args: {salvoes: Salvo[]}) {
  return (
    <div className="sensor-block">
      <div className="section-tag">Inbound</div>
      <ul className="salvo-list">
        {args.salvoes.map((salvo) => (
          <li key={salvo.source} className="salvo">
            <span className="salvo-count" title={`${salvo.count} inbound from ${salvo.source}`}>
              <MissileIcon />
              <span className="salvo-count-number">{salvo.count}</span>
            </span>
            <span className="salvo-source">from {salvo.source}</span>
            <span className="salvo-range">
              {formatKm(salvo.nearest)} km
              <span className="salvo-band"> · {bandName(salvo.nearest)}</span>
            </span>
            <span
              className={salvo.roundsOut <= 1 ? "salvo-eta salvo-eta-now" : "salvo-eta"}
              title="Rounds until the nearest one arrives, if nothing changes">
              {Number.isFinite(salvo.roundsOut) ? `${salvo.roundsOut}r` : "—"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * The squadron's picture: our ships across the top, every contact down the
 * side, a filled cell where that ship holds it.
 *
 * A row with a hole in it is a ship that cannot shoot at that contact and may
 * not know it is there -- the gap a hand-off exists to close, which is why the
 * grid is drawn rather than listed.
 */
function TeamPicture(args: {ours: string[]; rows: PictureRow[]}) {
  return (
    <div className="sensor-block">
      <div className="section-tag">Squadron picture</div>
      {args.rows.length === 0 ? (
        <p className="sensor-empty">Nobody on our side has found anything.</p>
      ) : (
        <table className="picture-grid">
          <thead>
            <tr>
              <th scope="col" className="picture-corner" />
              {args.ours.map((name) => (
                <th key={name} scope="col" className="picture-ship" title={name}>
                  {/* Initials: the grid has to stay narrow, and the full name
                      is a breath away in the tooltip. */}
                  {initials(name)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {args.rows.map((row) => (
              <tr key={row.contact} className={row.gap ? "picture-row picture-row-gap" : "picture-row"}>
                <th scope="row" className="picture-contact" style={{color: teamLabelColor(row.team)}}>
                  {row.contact}
                </th>
                {row.cells.map((cell) => (
                  <td key={cell.ship} className="picture-cell">
                    <PictureMark held={cell.held} lock={cell.lock} ship={cell.ship} contact={row.contact} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

/** Filled for held, ringed for a lock, hollow for a ship flying blind to it. */
function PictureMark(args: {held: boolean; lock: boolean; ship: string; contact: string}) {
  const title = args.lock
    ? `${args.ship} is locked onto ${args.contact}`
    : args.held
      ? `${args.ship} holds ${args.contact}`
      : `${args.ship} cannot see ${args.contact}`;
  return (
    <svg viewBox="0 0 16 16" className="picture-mark" role="img" aria-label={title}>
      <title>{title}</title>
      {args.held ? (
        <circle cx="8" cy="8" r="4" className="picture-mark-held" />
      ) : (
        <circle cx="8" cy="8" r="4" className="picture-mark-blind" />
      )}
      {args.lock && <circle cx="8" cy="8" r="7" className="picture-mark-lock" />}
    </svg>
  );
}

/**
 * What we know about each ship we have found: what it is, how big, and what it
 * is carrying.
 *
 * The sensop's reading of a contact. Only shown for ships this one has
 * actually detected -- a blip we have not resolved tells us nothing, and
 * saying so is part of the job.
 */
function ContactDetail(args: {
  observer: Ship;
  ships: Ship[];
  templates: {[name: string]: import("lib/shipDesignTemplates").ShipDesignTemplate};
}) {
  const contacts = args.ships
    .filter((ship) => ship.name !== args.observer.name)
    .filter((ship) => !isUndetected(args.observer, ship));

  if (contacts.length === 0) {
    return null;
  }

  return (
    <div className="sensor-block">
      <div className="section-tag">Contact detail</div>
      <ul className="contact-detail-list">
        {contacts.map((ship) => {
          const design = args.templates[ship.design];
          const weapons = design == null ? [] : shipWeapons(ship, args.templates);
          return (
            <li key={ship.name} className="contact-detail">
              <div className="contact-detail-head">
                <span className="contact-detail-name" style={{color: teamLabelColor(ship.team)}}>
                  {ship.name}
                </span>
                <span className="contact-detail-class">{ship.design}</span>
                {design != null && <span className="contact-detail-tons">{design.displacement} tons</span>}
              </div>
              <div className="contact-detail-line">
                {/* Thrust is what a pilot asks about first: what they can do
                    now, and what the design is rated for. */}
                <span className="contact-detail-label">thrust</span>
                <span>
                  {ship.current_maneuver}
                  {design != null && design.maneuver !== ship.current_maneuver && `(${design.maneuver})`}
                </span>
                <span className="contact-detail-label">sensors</span>
                <span>{ship.current_sensors}</span>
                {design?.stealth != null && (
                  <>
                    <span className="contact-detail-label">stealth</span>
                    <span>{design.stealth}</span>
                  </>
                )}
                {design?.countermeasures != null && (
                  <>
                    <span className="contact-detail-label">CM</span>
                    <span>{design.countermeasures}</span>
                  </>
                )}
              </div>
              {/* The armament in the gunner's own vocabulary: mount as the
                  shape, weapon as the colour, the full name on hover. */}
              <ul className="contact-weapons">
                {weapons.length === 0 ? (
                  <li className="contact-weapon contact-weapon-none">no armament on file</li>
                ) : (
                  weapons.map((weapon, index) => (
                    <li key={index} className="contact-weapon">
                      <WeaponGlyph weapon={weapon} />
                    </li>
                  ))
                )}
              </ul>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** "HMS Executor" -> "HE", for a column head two characters wide. */
const initials = (name: string): string =>
  name
    .split(/\s+/)
    .map((word) => word[0] ?? "")
    .join("")
    .slice(0, 2)
    .toUpperCase();

function EyeIcon() {
  return (
    <svg viewBox="0 0 16 16" className="sensor-icon" aria-hidden="true">
      <path d="M1 8 C4 3 12 3 15 8 C12 13 4 13 1 8 Z" />
      <circle cx="8" cy="8" r="2.5" className="sensor-icon-fill" />
    </svg>
  );
}

function CrosshairIcon() {
  return (
    <svg viewBox="0 0 16 16" className="sensor-icon sensor-icon-alarm" aria-hidden="true">
      <circle cx="8" cy="8" r="5" />
      <line x1="8" y1="0" x2="8" y2="4" />
      <line x1="8" y1="12" x2="8" y2="16" />
      <line x1="0" y1="8" x2="4" y2="8" />
      <line x1="12" y1="8" x2="16" y2="8" />
      <circle cx="8" cy="8" r="1.2" className="sensor-icon-fill" />
    </svg>
  );
}

function MissileIcon() {
  return (
    <svg viewBox="0 0 16 16" className="sensor-icon sensor-icon-alarm" aria-hidden="true">
      <path d="M14 8 L8 5 L2 5 L2 11 L8 11 Z" />
      <path d="M2 5 L0 2 L3 5 Z" className="sensor-icon-fill" />
      <path d="M2 11 L0 14 L3 11 Z" className="sensor-icon-fill" />
    </svg>
  );
}

export default SensorStation;
