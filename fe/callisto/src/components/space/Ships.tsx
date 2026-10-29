import * as React from "react";
import { memo, useEffect, useLayoutEffect, useMemo, useRef } from "react";

import { Group, InstancedMesh, Matrix4, Mesh, SphereGeometry, Vector3 as Vector3Three } from "three";
import {
  extend,
  useThree,
  useFrame,
  Vector3,
} from "@react-three/fiber";
import type { ThreeElement } from "@react-three/fiber";
import { TextGeometry, TextGeometryParameters } from "three/examples/jsm/geometries/TextGeometry";
import { FontLoader, Font } from "three/examples/jsm/loaders/FontLoader";

import { Text } from "@react-three/drei";
import { Line } from "lib/Util";

import {
  SCALE,
  TURN_IN_SECONDS,
  RANGE_BANDS,
  RANGE_BAND_NAMES
} from "lib/universal";
import { Ship as ShipType, Missile as MissileType } from "lib/entities";
import { missileCount } from "lib/missileCount";
import { isViewersOwn, undetectedTest } from "lib/contacts";
import { Box, isBox, selectableShips, shipsInBox } from "lib/boxSelect";
import { centroid } from "lib/squadrons";
import { missilePositions } from "lib/missileSwarm";
import { isReferee } from "lib/view";
import { useViewer } from "lib/viewer";
import { teamBodyColor, teamLabelColor } from "lib/teams";
import { FlightPath } from "lib/flightPath";

import { addVector, scaleVector, RangeCircle } from "lib/Util";

import { useAppSelector, useAppDispatch } from "state/hooks";
import {
  setEntityToShow,
  setComputerShipName,
  setFleetSelection,
  setFocusSquadron,
  tickShips,
} from "state/uiSlice";
import {entitiesSelector} from "state/serverSlice";

extend({ TextGeometry });

// Needed for some reason to make textGeometry work.
declare module '@react-three/fiber' {
  interface ThreeElements {
    textGeometry: ThreeElement<typeof TextGeometry>;
  }
}

let labelFont: Font | null = null;
new FontLoader().load(
  "/assets/Orbitron_Regular.json",
  (font) => {
    labelFont = font;
  },
  () => {},
  (error) => {
    console.log("Error loading Orbitron font: " + JSON.stringify(error));
  }
);

/**
 * Ships whose labels a squadron marker stands in for this frame, by name.
 *
 * Written by {@link SquadronMarkers} and read by each ship as the frame is
 * drawn, so a camera move hides and shows labels without re-rendering two
 * hundred components.
 */
const labelsUnderMarkers = new Set<string>();

/** How far from the camera, in scene units, a squadron becomes one marker.
 *  A ship's label is about 0.7 units tall, and past this it is a few pixels
 *  on a laptop screen. */
const MARKER_DISTANCE = 60;

function Ship(args: {
  ship: ShipType;
  index: number;
  undetected: boolean;
}) {
  // `showRange` holds the name of the one ship whose bands are being shown.
  // This compared it against `computerShipName` instead, which is the same
  // value in every copy of this component -- so every ship drew its own four
  // shells at once. Five ships a few thousand km apart, each with a 50,000 km
  // sphere, put twenty near-coincident surfaces in the same place, which is
  // what the display was actually showing.
  const showRange = useAppSelector(state => state.ui.showRange) === args.ship.name;
  const dispatch = useAppDispatch();

  // The ship whose console is open is the one doing the looking, or a fleet
  // commander's whole team. In the GM's all-ships view nobody is, so nothing
  // is dimmed and everything shows as it always did.
  const viewer = useViewer();
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const squadron = useAppSelector((state) => state.user.squadron);

  const isOwnShip = isViewersOwn(viewer, args.ship);
  const undetected = args.undetected;
  // Two axes, deliberately kept separate: hue says which side a ship is on,
  // brightness says how well this console can see it. Scaling the team colour
  // rather than replacing it keeps both readable at once -- a dimmed red ship
  // still reads as team red, not currently detected.
  //
  // Your own ship, and every ship in the GM's all-ships view, is at full
  // brightness; a ship you merely detect is dimmed; one you cannot see is
  // dimmer still, present only because the referee's table can see the board.
  const brightness = isOwnShip || viewer == null ? 1 : undetected ? 0.06 : 0.2;
  const bodyColor = teamBodyColor(args.ship.team, brightness);
  // Detection wins over team: a ship you cannot see reads grey whatever side it
  // is on. Once detected the team colour comes through, dimmed for anything
  // that is not the ship whose console is open.
  const labelColor = teamLabelColor(args.ship.team, {
    undetected,
    dim: !(isOwnShip || viewer == null),
  });

  const { camera } = useThree();
  const textRef = useRef<Mesh>(null);
  const shipRef = useRef<Mesh>(null);
  const textGeoRef = useRef<TextGeometry>(null);
  const shipGeoRef = useRef<SphereGeometry>(null);

  useFrame(() => {
    if (textRef.current != null) {
      textRef.current.visible = !labelsUnderMarkers.has(args.ship.name);
      textRef.current.lookAt(camera.position);
    }
  });

  // The label's geometry is rebuilt whenever its arguments change, and an
  // options object made afresh on each render counts as a change: every
  // render of every ship re-extruded its name. Two hundred of them on each
  // world update, and on each frame of a camera move, for nothing.
  const labelText = args.ship.controller === "AI" ? `${args.ship.name} [AI]` : args.ship.name;
  const labelArgs = useMemo(
    (): [string, TextGeometryParameters] | null =>
      labelFont == null ? null : [labelText, { font: labelFont, size: 0.7, depth: 0.05 }],
    // labelFont is set once, when the font has loaded; a ship drawn before
    // then gets her label on her next render.
    [labelText, labelFont],
  );
  function handleShipClick() {
    // A fleet commander takes the helm of their own team's ships only, a
    // squadron commander of their squadron's.
    if (fleetTeam != null && args.ship.team !== fleetTeam) {
      return;
    }
    if (squadron != null && !(args.ship.team === squadron.team && args.ship.squadron === squadron.name)) {
      return;
    }
    dispatch(setComputerShipName(args.ship.name));
  }

  return (
    <>
      {showRange &&
        RANGE_BANDS.map((distance, index) => (
          <RangeCircle
            pos={scaleVector(args.ship.position, SCALE)}
            distance={distance}
            // Named, so a band is identified by its label rather than by
            // counting rings outwards or reading a shade.
            label={RANGE_BAND_NAMES[index]}
            key={args.ship.name + "range" + index}
            color={"#5ba0ff"}
          />
        ))}
      <group position={scaleVector(args.ship.position, SCALE) as Vector3}>
        <mesh
          ref={shipRef}
          position={[0, 0, 0]}
          onPointerOver={() => dispatch(setEntityToShow(args.ship))}
          onPointerLeave={() => dispatch(setEntityToShow(null))}
          onClick={handleShipClick}>
          <sphereGeometry ref={shipGeoRef} args={[0.2]} />
          {/* HDR: the composer keeps a half-float buffer, so values above 1
              survive and set how hard a ship blooms relative to dimmer things
              like the labels. */}
          <meshBasicMaterial color={bodyColor} />
        </mesh>
        {/* vector showing a ships's velocity (so distance next turn) */}
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)}
          color="red"
        />
        {/* vector showing a ships's planed move in the next turn */}
        <Line
          start={scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)}
          end={addVector(
            scaleVector(
              args.ship.plan[0][0] as [number, number, number],
              SCALE * TURN_IN_SECONDS * TURN_IN_SECONDS
            ),
            scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)
          )}
          color="green"
        />
        {/* An AI-crewed ship says so where everyone looks: on the map. */}
        {labelArgs != null && (
          <mesh position={[0.0, -1.5, 0.0]} ref={textRef}>
            <textGeometry ref={textGeoRef} args={labelArgs} />
            <meshBasicMaterial attach="material" color={labelColor} />
          </mesh>
        )}
      </group>
    </>
  );
}

/**
 * Every ship, plus the squadron markers and box-select.
 *
 * Memoised: the scene's parent re-renders on every frame of a camera move,
 * since it holds the camera's position, and without this every ship re-drew
 * with it.
 */
export const Ships = memo(function Ships() {
  const entities = useAppSelector(entitiesSelector);
  const viewer = useViewer();
  // Worked out once for the whole fleet: asked ship by ship it read every
  // team-mate's contacts for every ship, which is a million look-ups at a
  // hundred a side.
  const undetected = useMemo(() => undetectedTest(viewer, entities.ships), [viewer, entities.ships]);

  return (
    <>
      {entities.ships.map((ship, index) => (
        <Ship
          key={ship.name}
          ship={ship}
          index={index}
          undetected={undetected(ship)}
        />
      ))}
      <SquadronMarkers />
      <BoxSelect />
    </>
  );
});

/** A squadron as its marker draws it. */
interface MarkerSquadron {
  key: string;
  team: string | null;
  name: string;
  ships: string[];
  centre: [number, number, number];
}

/**
 * One labelled marker per squadron when the camera is far from it, in place
 * of a label per ship: a hundred labels at that distance are a smear. Only
 * the squadrons the viewer is entitled to know of: every one for the
 * referee, their own side's for anyone else. Clicking a marker selects the
 * squadron, as the roster's Select does.
 */
function SquadronMarkers() {
  const entities = useAppSelector(entitiesSelector);
  const enabled = useAppSelector((state) => state.ui.squadronMarkers);
  const viewer = useViewer();
  const roles = useAppSelector((state) => state.user.roles);
  const shipName = useAppSelector((state) => state.user.shipName);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const dispatch = useAppDispatch();
  const canSelect = fleetTeam != null || isReferee(roles, shipName);

  const squadrons: MarkerSquadron[] = useMemo(() => {
    const team = viewer == null ? undefined : "ship" in viewer ? viewer.ship.team ?? null : viewer.team;
    const groups = new Map<string, ShipType[]>();
    entities.ships
      .filter((ship) => ship.squadron != null && (team === undefined || (ship.team ?? null) === team))
      .forEach((ship) => {
        const key = `${ship.team ?? ""}\u0000${ship.squadron}`;
        groups.set(key, [...(groups.get(key) ?? []), ship]);
      });
    return [...groups.entries()].map(([key, ships]) => ({
      key,
      team: ships[0].team ?? null,
      name: ships[0].squadron as string,
      ships: ships.map((ship) => ship.name),
      centre: scaleVector(centroid(ships), SCALE) as [number, number, number],
    }));
  }, [entities.ships, viewer]);

  if (!enabled) {
    return null;
  }
  return (
    <>
      {squadrons.map((squadron) => (
        <SquadronMarker
          key={squadron.key}
          squadron={squadron}
          onSelect={
            canSelect
              ? () => {
                  dispatch(setFleetSelection(squadron.ships));
                  dispatch(setFocusSquadron({ team: squadron.team, name: squadron.name }));
                }
              : undefined
          }
        />
      ))}
    </>
  );
}

function SquadronMarker(args: { squadron: MarkerSquadron; onSelect?: () => void }) {
  const { camera } = useThree();
  const group = useRef<Group>(null);
  const label = useRef<Group>(null);
  const { squadron } = args;
  const colour = teamLabelColor(squadron.team as ShipType["team"]);

  // A marker switched off, or a squadron gone or changed, gives its ships
  // their labels back.
  const members = squadron.ships.join("\u0000");
  useEffect(() => () => members.split("\u0000").forEach((name) => labelsUnderMarkers.delete(name)), [members]);

  useFrame(() => {
    if (group.current == null) {
      return;
    }
    const distance = camera.position.distanceTo(group.current.position);
    const far = distance > MARKER_DISTANCE;
    group.current.visible = far;
    squadron.ships.forEach((name) => (far ? labelsUnderMarkers.add(name) : labelsUnderMarkers.delete(name)));
    if (far) {
      // The same size on the screen however far away it is.
      group.current.scale.setScalar(distance / MARKER_DISTANCE);
      label.current?.lookAt(camera.position);
    }
  });

  return (
    <group ref={group} position={squadron.centre as Vector3} visible={false}>
      <mesh
        onClick={(event) => {
          if (args.onSelect != null && group.current?.visible) {
            event.stopPropagation();
            args.onSelect();
          }
        }}>
        <octahedronGeometry args={[0.8]} />
        <meshBasicMaterial color={colour} wireframe={true} />
      </mesh>
      <group ref={label}>
        <Text color={colour} fontSize={1.2} position={[0, -2, 0]} anchorY="top">
          {`${squadron.name} (${squadron.ships.length})`}
        </Text>
      </group>
    </group>
  );
}

/**
 * Shift-drag in the view to tick every ship inside the box for the
 * Squadrons panel's Move; Shift-Ctrl (or Shift-⌘) drag adds to the ticks.
 * For the referee and a fleet commander, who are the ones making squadrons;
 * a fleet commander ticks only their own side. Listens before the camera
 * controls do, so a box drag does not also swing the view.
 */
function BoxSelect() {
  const { camera, size } = useThree();
  const entities = useAppSelector(entitiesSelector);
  const roles = useAppSelector((state) => state.user.roles);
  const shipName = useAppSelector((state) => state.user.shipName);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const dispatch = useAppDispatch();
  const referee = isReferee(roles, shipName);
  const allowed = referee || fleetTeam != null;

  // The latest of everything the listeners read, so they need not be
  // re-attached on every world update.
  const latest = useRef({ ships: entities.ships, referee, fleetTeam, camera, size });
  latest.current = { ships: entities.ships, referee, fleetTeam, camera, size };

  useEffect(() => {
    const container = document.getElementById("main-canvas");
    if (!allowed || container == null) {
      return undefined;
    }
    let start: { x: number; y: number; add: boolean } | null = null;
    const rect = document.createElement("div");
    rect.className = "box-select";
    rect.style.display = "none";
    document.body.appendChild(rect);

    const draw = (box: Box) => {
      rect.style.display = "block";
      rect.style.left = `${Math.min(box.x0, box.x1)}px`;
      rect.style.top = `${Math.min(box.y0, box.y1)}px`;
      rect.style.width = `${Math.abs(box.x1 - box.x0)}px`;
      rect.style.height = `${Math.abs(box.y1 - box.y0)}px`;
    };
    const move = (event: PointerEvent) => {
      if (start != null) {
        draw({ x0: start.x, y0: start.y, x1: event.clientX, y1: event.clientY });
      }
    };
    const up = (event: PointerEvent) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      rect.style.display = "none";
      if (start == null) {
        return;
      }
      const box = { x0: start.x, y0: start.y, x1: event.clientX, y1: event.clientY };
      const add = start.add;
      start = null;
      if (!isBox(box)) {
        return;
      }
      const { ships, referee, fleetTeam, camera, size } = latest.current;
      const bounds = container.getBoundingClientRect();
      const toScreen = (position: [number, number, number]): [number, number] | null => {
        const at = new Vector3Three(position[0] * SCALE, position[1] * SCALE, position[2] * SCALE).project(camera);
        if (at.z > 1) {
          return null;
        }
        return [bounds.left + ((at.x + 1) / 2) * size.width, bounds.top + ((1 - at.y) / 2) * size.height];
      };
      const inBox = new Set(shipsInBox(selectableShips(ships, referee, fleetTeam), toScreen, box));
      const boxed = ships.filter((ship) => inBox.has(ship.name));
      // Squadrons are made within a team, so a referee's box keeps the team
      // it caught most of.
      const counts = new Map<string | null, number>();
      boxed.forEach((ship) => counts.set(ship.team ?? null, (counts.get(ship.team ?? null) ?? 0) + 1));
      const team = [...counts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
      const names = boxed.filter((ship) => (ship.team ?? null) === team).map((ship) => ship.name);
      dispatch(add ? tickShips({ ships: names, on: true }) : setFleetSelection(names));
    };
    const down = (event: PointerEvent) => {
      if (!event.shiftKey || event.button !== 0) {
        return;
      }
      // Ours, not the camera's: it would turn the view as the box is drawn.
      event.stopPropagation();
      event.preventDefault();
      start = { x: event.clientX, y: event.clientY, add: event.ctrlKey || event.metaKey };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    };
    container.addEventListener("pointerdown", down, { capture: true });
    return () => {
      container.removeEventListener("pointerdown", down, { capture: true });
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      rect.remove();
    };
  }, [allowed, dispatch]);

  return null;
}

export function Missile(args: { missile: MissileType; index: number }) {
  const labelRef = useRef<Group>(null);
  const dispatch = useAppDispatch();

  return (
    <>
      <group
        ref={labelRef}
        position={scaleVector(args.missile.position, SCALE) as Vector3}>
        <mesh
          position={[0, 0, 0]}
          onPointerOver={() => dispatch(setEntityToShow(args.missile))}
          onPointerLeave={() => dispatch(setEntityToShow(null))}>
          <sphereGeometry args={[0.1]} />
          <meshBasicMaterial color={[8.0, 0, 0]} />
        </mesh>
        {/* vector showing a missile's velocity (so distance next turn) */}
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)}
          color="grey"
        />
        {/* vector showing a missile's planed move in the next turn */}
        <Line
          start={scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)}
          end={addVector(
            scaleVector(
              args.missile.acceleration,
              SCALE * TURN_IN_SECONDS * TURN_IN_SECONDS
            ),
            scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)
          )}
          color="green"
        />
        <Text color="grey" fontSize={0.2} position={[0, -0.1, 0]}>
          {missileCount(args.missile) > 1
            ? `${args.missile.name} ×${missileCount(args.missile)}`
            : args.missile.name}
        </Text>
      </group>
    </>
  );
}

/**
 * Above this many missiles in flight they are drawn as one swarm, without
 * names or vectors. Each missile drawn on its own is a sphere, two lines and
 * a text label: thousands of them is thousands of draw calls and text
 * meshes, which no laptop draws at a usable rate.
 */
export const MISSILE_DETAIL_LIMIT = 300;

/** Memoised for the reason {@link Ships} is. */
export const Missiles = memo(function Missiles() {
  const entities = useAppSelector(entitiesSelector);

  if (entities.missiles.length > MISSILE_DETAIL_LIMIT) {
    return <MissileSwarm key={entities.missiles.length} missiles={entities.missiles} />;
  }
  return (
    <>
      {entities.missiles.map((missile, index) => (
        <Missile key={missile.name} missile={missile} index={index} />
      ))}
    </>
  );
});

/** A swarm of missiles as one instanced mesh: one draw call however many.
 *  Keyed on the count by its caller, since an instanced mesh's size is fixed
 *  when it is made. */
function MissileSwarm(args: { missiles: MissileType[] }) {
  const mesh = useRef<InstancedMesh>(null);
  useLayoutEffect(() => {
    const swarm = mesh.current;
    if (swarm == null) {
      return;
    }
    const at = missilePositions(args.missiles);
    const place = new Matrix4();
    for (let i = 0; i < args.missiles.length; i++) {
      place.makeTranslation(at[i * 3], at[i * 3 + 1], at[i * 3 + 2]);
      swarm.setMatrixAt(i, place);
    }
    swarm.instanceMatrix.needsUpdate = true;
    swarm.computeBoundingSphere();
  }, [args.missiles]);

  return (
    <instancedMesh ref={mesh} args={[undefined, undefined, args.missiles.length]} frustumCulled={false}>
      <sphereGeometry args={[0.1, 6, 4]} />
      <meshBasicMaterial color={[8.0, 0, 0]} />
    </instancedMesh>
  );
}

export function Route(args: { plan: FlightPath }) {
  const start = scaleVector(args.plan.path[0], -1.0 * SCALE);
  let prev = args.plan.path[0];
  const path = args.plan.path.slice(1);

  return (
    <group position={scaleVector(prev, SCALE) as Vector3}>
      {path.map((point, index) => {
        const oldPoint = prev;
        prev = point;
        return (
          <Line
            key={index}
            start={addVector(start, scaleVector(oldPoint, SCALE))}
            end={addVector(start, scaleVector(point, SCALE))}
            color={"orange"}
          />
        );
      })}
    </group>
  );
}
