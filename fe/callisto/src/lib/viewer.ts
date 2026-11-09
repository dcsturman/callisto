import { useMemo } from "react";
import { Viewer } from "lib/contacts";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

/**
 * Who this player is looking as: their ship, if they hold one; their team,
 * as Fleet Command or Squadron Command; or nobody in particular, as the referee or a player at
 * no console, who sees the whole board.
 */
export const useViewer = (): Viewer => {
  const shipName = useAppSelector((state) => state.user.shipName);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const squadron = useAppSelector((state) => state.user.squadron);
  const entities = useAppSelector(entitiesSelector);
  return useMemo(() => {
    if (fleetTeam != null) {
      return { team: fleetTeam };
    }
    // A squadron's commander sees as her team does: team-mates share a plot.
    if (squadron?.team != null) {
      return { team: squadron.team };
    }
    const ship = shipName == null ? null : entities.ships.find((s) => s.name === shipName);
    return ship == null ? null : { ship };
  }, [fleetTeam, squadron, shipName, entities.ships]);
};
