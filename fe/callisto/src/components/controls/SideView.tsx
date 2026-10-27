import * as React from "react";
import { useState } from "react";
import { Entity } from "lib/entities";
import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import { sideView } from "lib/bearing";
import { useAppDispatch, useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";
import { setCameraPos, setCameraQuaternion } from "state/uiSlice";

/** The camera's field of view, as the scene sets it up in App.tsx. */
const FIELD_OF_VIEW = 75;

const FILTER = [EntitySelectorType.Ship, EntitySelectorType.Planet];

/**
 * Put the camera side-on to the line between two things, to judge the
 * distance between them by eye: a ship and a planet's 100-diameter limit,
 * two ships closing. Any view along the line flattens it.
 */
export function SideView() {
  const dispatch = useAppDispatch();
  const entities = useAppSelector(entitiesSelector);
  const [first, setFirst] = useState<string | null>(null);
  const [second, setSecond] = useState<string | null>(null);

  const where = (name: string | null) =>
    name == null
      ? undefined
      : [...entities.ships, ...entities.planets].find((entity) => entity.name === name)?.position;
  const a = where(first);
  const b = where(second);

  const look = () => {
    if (a == null || b == null) return;
    const view = sideView(a, b, FIELD_OF_VIEW);
    dispatch(setCameraPos({ x: view.position[0], y: view.position[1], z: view.position[2] }));
    dispatch(setCameraQuaternion(view.quaternion));
  };

  return (
    <div className="side-view" data-tutorial="side-view">
      <div className="side-view-label" title="Look side-on at the line between two ships or planets">
        Side view
      </div>
      <div className="side-view-row">
        <EntitySelector
          filter={FILTER}
          current={first}
          setChoice={(entity: Entity | null) => setFirst(entity?.name ?? null)}
          exclude={second ?? undefined}
          aria-label="Side view: first"
        />
        <EntitySelector
          filter={FILTER}
          current={second}
          setChoice={(entity: Entity | null) => setSecond(entity?.name ?? null)}
          exclude={first ?? undefined}
          aria-label="Side view: second"
        />
        <button
          type="button"
          className="control-input control-button blue-button"
          disabled={a == null || b == null}
          onClick={look}>
          Look
        </button>
      </div>
    </div>
  );
}
