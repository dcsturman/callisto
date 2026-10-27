import * as React from "react";
import { useMemo, useState } from "react";
import { CUSTOMISABLE_ROLES, ViewMode, rolesToString } from "lib/view";
import { requestRoleChoice, requestFleetCommand } from "lib/serverManager";
import { TEAMS } from "lib/teams";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { setRoleShip } from "state/userSlice";
import { entitiesSelector } from "state/serverSlice";

const CUSTOM = "custom";
/** The seat chooser's value for Fleet Command of a team: `fleet:Blue`. */
const FLEET = "fleet:";

export const RoleChooser = () => {
  const shipName = useAppSelector((state) => state.user.shipName);
  const roles = useAppSelector((state) => state.user.roles);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const entities = useAppSelector(entitiesSelector);
  const [picking, setPicking] = useState(false);
  const dispatch = useAppDispatch();

  // A Fleet Command seat for every team with a ship on it. Unaligned ships
  // have no team to command; their crews just take the ship.
  const teams = useMemo(
    () => TEAMS.filter((team) => entities.ships.some((ship) => ship.team === team)),
    [entities.ships],
  );

  const choose = useMemo(
    () => (next: ViewMode[], ship: string | null) => {
      dispatch(setRoleShip([next, ship]));
      requestRoleChoice(next, ship);
    },
    [dispatch],
  );

  // Leaving Fleet Command for a ship or the GM's seat takes General: the
  // commander's role means nothing at one console.
  const seatRoles = roles.includes(ViewMode.FleetCommand) ? [ViewMode.General] : roles;
  const chooseSeat = (value: string) => {
    if (value.startsWith(FLEET)) {
      requestFleetCommand(value.slice(FLEET.length));
    } else {
      choose(seatRoles, value === "" ? null : value);
    }
  };
  const seat = fleetTeam != null ? FLEET + fleetTeam : shipName ?? "";

  // A single role shows as itself. More than one shows as Custom, since no
  // single option describes it -- the dialog is where the detail lives.
  const selectValue = roles.length === 1 ? String(roles[0]) : CUSTOM;

  return (
    <>
      <select
        className="select-dropdown control-name-input control-input role-input"
        data-tutorial="seat-menu"
        aria-label="Seat"
        value={seat}
        onChange={(e) => chooseSeat(e.target.value)}
      >
        {/* Taking no ship means running the whole board rather than declining
            to choose, so the empty option says so. */}
        <option value="">GM</option>
        {teams.map((team) => (
          <option key={team} value={FLEET + team}>
            Fleet Command ({team})
          </option>
        ))}
        {entities.ships.map((ship) => (
          <option key={ship.name} value={ship.name}>
            {ship.name}
          </option>
        ))}
      </select>
      <select
        className="select-dropdown control-name-input control-input role-input"
        data-tutorial="role-menu"
        // A fleet commander works every station of whichever team ship they
        // have open; there is no single station to pick.
        disabled={fleetTeam != null}
        value={fleetTeam != null ? String(ViewMode.FleetCommand) : selectValue}
        title={roles.length > 1 ? rolesToString(roles) : undefined}
        onChange={(e) => {
          if (e.target.value === CUSTOM) {
            setPicking(true);
            return;
          }
          choose([Number(e.target.value)], shipName);
        }}
      >
        <option value={ViewMode.General}>General</option>
        <option value={ViewMode.Pilot}>Pilot</option>
        <option value={ViewMode.Sensors}>Sensors</option>
        <option value={ViewMode.Gunner}>Gunner</option>
        <option value={ViewMode.Engineer}>Engineer</option>
        <option value={ViewMode.Observer}>Observer</option>
        <option value={ViewMode.Captain}>Captain</option>
        {fleetTeam != null && <option value={ViewMode.FleetCommand}>Fleet Command</option>}
        {/* Several stations at once, for a small crew. Shown as the chosen
            names once picked, since no single option describes it. */}
        <option value={CUSTOM}>
          {roles.length > 1 ? rolesToString(roles) : "Custom\u2026"}
        </option>
      </select>
      {picking && (
        <RolePicker
          initial={roles.filter((r) => CUSTOMISABLE_ROLES.includes(r))}
          onCancel={() => setPicking(false)}
          onApply={(next) => {
            setPicking(false);
            choose(next, shipName);
          }}
        />
      )}
    </>
  );
};

/**
 * Pick several stations. General and Observer are not offered: General is
 * every station already and Observer is none, so neither combines with
 * anything. Styled on the confirm dialog so it reads as the same kind of
 * thing; Escape cancels, and Apply needs at least one station.
 */
function RolePicker(args: {
  initial: ViewMode[];
  onCancel: () => void;
  onApply: (roles: ViewMode[]) => void;
}) {
  const [chosen, setChosen] = useState<ViewMode[]>(args.initial);

  React.useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        args.onCancel();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [args]);

  const toggle = (role: ViewMode) =>
    setChosen((prev) =>
      prev.includes(role)
        ? prev.filter((r) => r !== role)
        : // Keep the fixed station order rather than click order, so two
          // players who pick the same pair read the same in the user list.
          CUSTOMISABLE_ROLES.filter((r) => r === role || prev.includes(r)),
    );

  return (
    <div
      className="confirm-dialog-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          args.onCancel();
        }
      }}
    >
      <div className="confirm-dialog role-picker" role="dialog" aria-modal="true" aria-labelledby="role-picker-title">
        <h2 id="role-picker-title">Stations</h2>
        <p className="confirm-dialog-message">
          Pick every station you are working on this ship.
        </p>
        <div className="role-picker-options">
          {CUSTOMISABLE_ROLES.map((role) => (
            <label key={role} className="role-picker-option">
              <input
                type="checkbox"
                checked={chosen.includes(role)}
                onChange={() => toggle(role)}
              />
              {ViewMode[role]}
            </label>
          ))}
        </div>
        <div className="confirm-dialog-button-row">
          <button type="button" onClick={args.onCancel}>
            Cancel
          </button>
          <button
            type="button"
            disabled={chosen.length === 0}
            onClick={() => args.onApply(chosen)}
          >
            Apply
          </button>
        </div>
      </div>
    </div>
  );
}
