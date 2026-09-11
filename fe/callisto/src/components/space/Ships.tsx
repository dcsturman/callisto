import * as React from "react";
import { useRef, useMemo } from "react";

import { Group, Mesh, SphereGeometry } from "three";
import {
  extend,
  useThree,
  useFrame,
  Vector3,
} from "@react-three/fiber";
import type { ThreeElement } from "@react-three/fiber";
import { TextGeometry } from "three/examples/jsm/geometries/TextGeometry";
import { FontLoader, Font } from "three/examples/jsm/loaders/FontLoader";

import { Text } from "@react-three/drei";
import { Line } from "lib/Util";

import {
  SCALE,
  TURN_IN_SECONDS,
  RANGE_BANDS
} from "lib/universal";
import { Ship as ShipType, Missile as MissileType} from "lib/entities";
import { isUndetected } from "lib/contacts";
import { teamBodyColor, TEAM_CSS } from "lib/teams";
import { FlightPath } from "lib/flightPath";

import { addVector, scaleVector, RangeSphere } from "lib/Util";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { setEntityToShow, setComputerShipName } from "state/uiSlice";
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

function Ship(args: {
  ship: ShipType;
  index: number;
}) {
  const computerShipName = useAppSelector(state => state.ui.computerShipName);
  const showRange = useAppSelector(state => state.ui.showRange) === computerShipName;
  const dispatch = useAppDispatch();

  // The ship whose console is open is the one doing the looking. In the GM's
  // all-ships view no ship is being flown, so nothing is dimmed and everything
  // shows as it always did.
  const viewingShipName = useAppSelector((state) => state.user.shipName);
  const entities = useAppSelector(entitiesSelector);
  const observer = useMemo(
    () =>
      viewingShipName == null
        ? null
        : entities.ships.find((s) => s.name === viewingShipName) ?? null,
    [entities.ships, viewingShipName],
  );

  const isOwnShip = viewingShipName === args.ship.name;
  const undetected = isUndetected(observer, args.ship.name);
  // Two axes, deliberately kept separate: hue says which side a ship is on,
  // brightness says how well this console can see it. Scaling the team colour
  // rather than replacing it keeps both readable at once -- a dimmed red ship
  // still reads as team red, not currently detected.
  //
  // Your own ship, and every ship in the GM's all-ships view, is at full
  // brightness; a ship you merely detect is dimmed; one you cannot see is
  // dimmer still, present only because the referee's table can see the board.
  const brightness = isOwnShip || observer == null ? 1 : undetected ? 0.06 : 0.2;
  const bodyColor = teamBodyColor(args.ship.team, brightness);
  const labelColor =
    isOwnShip || observer == null
      ? args.ship.team
        ? TEAM_CSS[args.ship.team]
        : "#3dfc32"
      : undetected
        ? "#5a5a5a"
        : "#9a9a9a";

  const { camera } = useThree();
  const textRef = useRef<Mesh>(null);
  const shipRef = useRef<Mesh>(null);
  const textGeoRef = useRef<TextGeometry>(null);
  const shipGeoRef = useRef<SphereGeometry>(null);

  useFrame(() => {
    textRef.current?.lookAt(camera.position);
  });
  function handleShipClick() {
    dispatch(setComputerShipName(args.ship.name));
  }

  return (
    <>
      {computerShipName && showRange && RANGE_BANDS.map(
          (distance, index) => (
              <RangeSphere
                pos={scaleVector(args.ship.position, SCALE)}
                distance={distance}
                order={2*index}
                key={showRange + "range" + index}
                color={"#5ba0ff"}
                opacity={0.18}
              />
          )
        )}
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
        {labelFont != null && (
          <mesh position={[0.0, -1.5, 0.0]} ref={textRef}>
            <textGeometry
              ref={textGeoRef}
              args={[
                args.ship.name,
                { font: labelFont, size: 0.7, depth: 0.05 },
              ]}
            />
            <meshBasicMaterial attach="material" color={labelColor} />
          </mesh>
        )}
      </group>
    </>
  );
}

export function Ships() {
  const entities = useAppSelector(entitiesSelector);

  return (
    <>
      {entities.ships.map((ship, index) => (
        <Ship
          key={ship.name}
          ship={ship}
          index={index}
        />
      ))}
    </>
  );
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
          {args.missile.name}
        </Text>
      </group>
    </>
  );
}

export function Missiles() {
  const entities = useAppSelector(entitiesSelector);

  return (
    <>
      {entities.missiles.map((missile, index) => (
        <Missile key={missile.name} missile={missile} index={index} />
      ))}
    </>
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
