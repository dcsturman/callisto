import { useContext, useRef } from "react";

import { Group, Mesh, SphereGeometry, } from "three";
import {
  extend,
  ReactThreeFiber,
  useThree,
  useFrame,
  Vector3,
} from "@react-three/fiber";
import { TextGeometry } from "three/examples/jsm/geometries/TextGeometry";
import { FontLoader, Font } from "three/examples/jsm/loaders/FontLoader";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "./Util";

import {
  EntitiesServerContext,
  FlightPathResult,
  Ship as ShipType,
  Missile as MissileType,
} from "./Universal";

import { SCALE, TURN_IN_SECONDS, EntityToShowContext } from "./Universal";
import { addVector, scaleVector } from "./Util";

extend({ TextGeometry });

// This is to make typescript work with "extend"
declare global {
  namespace JSX {
    interface IntrinsicElements {
      textGeometry: ReactThreeFiber.Object3DNode<
        TextGeometry,
        typeof TextGeometry
      >;
    }
  }
}

let labelFont: Font | null = null;
new FontLoader().load(
  "/assets/Orbitron_Regular.json",
  (font) => {
    console.log("(Ships) Loaded Orbitron font.");
    labelFont = font;
  },
  () => {},
  (error) => {
    console.log("Error loading Orbitron font: " + JSON.stringify(error));
  }
);

//TODO: Move this somewhere else - maybe Controls.tsx

function Ship(args: {
  ship: ShipType;
  index: number;
  setComputerShipName: (shipName: string | null) => void;
}) {
  const entityToShow = useContext(EntityToShowContext);
  const { camera } = useThree();
  const textRef = useRef<Mesh>(null);
  const shipRef = useRef<Mesh>(null);
  const textGeoRef = useRef<TextGeometry>(null);
  const shipGeoRef = useRef<SphereGeometry>(null);

  useFrame(() => {
    textRef.current?.lookAt(camera.position);
  });
  function handleShipClick() {
    args.setComputerShipName(args.ship.name);
  }

  return (
    <>
      {
        <EffectComposer>
          <Bloom
            mipmapBlur
            luminanceThreshold={1}
            luminanceSmoothing={1}
            intensity={1.0}
          />
        </EffectComposer>
      }
      <group
        position={scaleVector(args.ship.position, SCALE) as Vector3}>
        <mesh
          ref={shipRef}
          position={[0, 0, 0]}
          onPointerOver={() => entityToShow.setEntityToShow(args.ship)}
          onPointerLeave={() => entityToShow.setEntityToShow(null)}
          onClick={handleShipClick}>
          <sphereGeometry ref={shipGeoRef} args={[0.2]} />
          <meshBasicMaterial color={[3, 3, 8.0]} />
        </mesh>
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)}
          color="red"
        />
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
            <meshBasicMaterial attach="material" color="#3dfc32" />
          </mesh>
        )}
      </group>
    </>
  );
}

export function Ships(args: {
  setComputerShipName: (shipName: string | null) => void;
}) {
  const serverEntities = useContext(EntitiesServerContext);
  return (
    <>
      {serverEntities.entities.ships.map((ship, index) => (
        <Ship
          key={ship.name}
          ship={ship}
          index={index}
          setComputerShipName={args.setComputerShipName}
        />
      ))}
    </>
  );
}

export function Missile(args: { missile: MissileType; index: number }) {
  const entityToShow = useContext(EntityToShowContext);
  const labelRef = useRef<Group>(null);

  console.log("(Ships.Missile) Missile: " + JSON.stringify(args.missile));

  return (
    <>
      <EffectComposer>
        <Bloom
          mipmapBlur
          luminanceThreshold={1}
          luminanceSmoothing={1}
          intensity={5.0}
        />
      </EffectComposer>
      <group
        ref={labelRef}
        position={scaleVector(args.missile.position, SCALE) as Vector3}>
        <mesh
          position={[0, 0, 0]}
          onPointerOver={() => entityToShow.setEntityToShow(args.missile)}
          onPointerLeave={() => entityToShow.setEntityToShow(null)}>
          <sphereGeometry args={[0.05]} />
          <meshBasicMaterial color={[8.0, 0, 0]} />
        </mesh>
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)}
          color="grey"
        />
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
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.entities.missiles.map((missile, index) => (
        <Missile key={missile.name} missile={missile} index={index} />
      ))}
    </>
  );
}

export function Route(args: { plan: FlightPathResult }) {
  console.log(`(Ships.Route) Display plan: ${JSON.stringify(args.plan)}`);
  console.log(`(Ships.Route) Display route: ${JSON.stringify(args.plan.path)}`);
  let start = scaleVector(args.plan.path[0], -1.0 * SCALE);
  let prev = args.plan.path[0];
  let path = args.plan.path.slice(1);

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

// Validate a USP string to see if its structurally correct.
// Could just return a boolean but we return a boolean if correct and then an optional string
// if there is an error message.
export function validateUSP(usp: string): [boolean, string | null] {
  if (usp.length !== 13+2) {
    return [false, `USP must be 13 characters long.  Provided string is ${usp.replace(/-/g, "").length} characters long.`];
  } else if (usp[7] !== "-") {
    return [false, `USP must have a dash at position 8.  Provided string is ${usp}.`];
  } else if (usp[13] !== "-") {
    return [false, `USP must have a dash at position 14.  Provided string is ${usp} with ${usp[14]} at position 14.`];
  } else if (usp.indexOf("O") !== -1) {
    return [false, `USP has an O.  Maybe you intended a zero? Provided string is ${usp}.`];
  } else if (usp.indexOf("I") !== -1) {
    return [false, `USP has an I.  Maybe you intended a one? Provided string is ${usp}.`];
  } else if (usp.toUpperCase() !== usp) {
    return [false, `USP must only use uppercase.  Provided string is ${usp}.`];
  }
  return [true, null];
}
