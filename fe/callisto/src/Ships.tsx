import { useContext, useRef } from "react";

import { Group } from "three";
import { Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "./Util";

import {
  Entity,
  EntitiesServerContext,
  FlightPathResult,
  Planet as PlanetType,
  Ship as ShipType,
  Missile as MissileType,
} from "./Universal";
import { Vector3 } from "@react-three/fiber";

import { SCALE, TURN_IN_SECONDS, EntityToShowContext } from "./Universal";
import { addVector, scaleVector, vectorToString } from "./Util";

//TODO: Move this somewhere else - maybe Controls.tsx
export function EntityInfoWindow(args: { entity: Entity }) {
  let isPlanet = false;
  let isShip = false;
  let ship_next_accel: [number, number, number] = [0, 0, 0];
  let radiusKm = 0;

  if (args.entity instanceof PlanetType) {
    isPlanet = true;
    radiusKm = args.entity.radius / 1000.0;
  } else if (args.entity instanceof ShipType) {
    isShip = true;
    ship_next_accel = args.entity.plan[0][0];
  }

  return (
    <div id="ship-info-window" className="ship-info-window">
      <h2 className="ship-info-title">{args.entity.name}</h2>
      <div className="ship-info-content">
        <p>
          Position (km):{" "}
          {vectorToString(scaleVector(args.entity.position, 1e-3))}
        </p>
        <p>Velocity (m/s): {vectorToString(args.entity.velocity)}</p>
        {isPlanet ? (
          <p>Radius (km): {radiusKm}</p>
        ) : isShip ? (
          <p> Acceleration (G): {vectorToString(ship_next_accel)}</p>
        ) : (
          <></>
        )}
      </div>
    </div>
  );
}

function Ship(args: {
  ship: ShipType;
  index: number;
  setComputerShip: (ship: ShipType | null) => void;
}) {
  const entityToShow = useContext(EntityToShowContext);
  const labelRef = useRef<Group>(null);

  function handleShipClick() {
    args.setComputerShip(args.ship);
  }

  return (
    <>
      {
        <Bloom
          mipmapBlur
          luminanceThreshold={1}
          luminanceSmoothing={1}
          intensity={5.0}
        />
      }
      <group
        ref={labelRef}
        position={scaleVector(args.ship.position, SCALE) as Vector3}>
        <mesh
          position={[0, 0, 0]}
          onPointerOver={() => entityToShow.setEntityToShow(args.ship)}
          onPointerLeave={() => entityToShow.setEntityToShow(null)}
          onClick={handleShipClick}>
          <sphereGeometry args={[0.1]} />
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
        <Text color="grey" fontSize={0.2} position={[0, -0.1, 0]}>
          {args.ship.name}
        </Text>
      </group>
    </>
  );
}

export function Ships(args: {
  setComputerShip: (ship: ShipType | null) => void;
}) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.entities.ships.map((ship, index) => (
        <Ship
          key={ship.name}
          ship={ship}
          index={index}
          setComputerShip={args.setComputerShip}
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
      {
        <Bloom
          mipmapBlur
          luminanceThreshold={1}
          luminanceSmoothing={1}
          intensity={5.0}
        />
      }
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
