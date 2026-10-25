import { useMemo } from "react";
import { Viewer } from "lib/contacts";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

/**
 * Who this player is looking as: their ship, if they hold one; their team,
 * as Fleet Command; or nobody in particular, as the referee or a player at
 * no console, who sees the whole board.
 */
export const useViewer = (): Viewer => {
  const shipName = useAppSelector((state) => state.user.shipName);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const entities = useAppSelector(entitiesSelector);
  return useMemo(() => {
    if (fleetTeam != null) {
      return { team: fleetTeam };
    }
    const ship = shipName == null ? null : entities.ships.find((s) => s.name === shipName);
    return ship == null ? null : { ship };
  }, [fleetTeam, shipName, entities.ships]);
};
