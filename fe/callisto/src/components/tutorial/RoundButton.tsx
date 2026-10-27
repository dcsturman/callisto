import * as React from "react";
import { isReferee } from "lib/view";
import { useAppSelector } from "state/hooks";

/**
 * The name of the button that ends the round, as this player has it: the GM
 * has Next Round, and a player holding a ship has Ready. A lesson can be
 * played either way, so it names whichever is on the screen.
 */
export function RoundButton() {
  const roles = useAppSelector((state) => state.user.roles);
  const shipName = useAppSelector((state) => state.user.shipName);
  return <b>{isReferee(roles, shipName) ? "Next Round" : "Ready"}</b>;
}
