import { useContext, useRef } from "react";

import { Group } from "three";
import { Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "./Util";

import { Entity, EntitiesServerContext, FlightPlan, Planet as PlanetType } from "./Contexts";
import { Vector3 } from "@react-three/fiber";

import { SCALE, TURN_IN_SECONDS, EntityToShowContext } from "./Contexts";
import { addVector, scaleVector, vectorToString } from "./Util";

//TODO: Move this somewhere else - maybe Controls.tsx
export function EntityInfoWindow(args: { entity: Entity}) {
  let isPlanet = false;
  console.log("isPlanet: " + isPlanet);
  let planet_details = null;
  let radiusKm = 0;
  if (args.entity.kind !== "Ship" && "Planet" in args.entity.kind) {
    isPlanet = true;
    planet_details = args.entity.kind.Planet as PlanetType;
    radiusKm = planet_details.radius / 1000.0;
    console.log("Planet details: " + JSON.stringify(planet_details));
    console.log("radius " + planet_details.color);
  }

  return (
    <div id="ship-info-window" className="ship-info-window">
      <h2 className="ship-info-title">{args.entity.name}</h2>
      <div className="ship-info-content">
        <p>Position (km): {vectorToString(scaleVector(args.entity.position,1e-3))}</p>
        <p>Velocity (m/s): {vectorToString(args.entity.velocity)}</p>
        { isPlanet ? <p>Radius (km): {radiusKm}</p> : <p> Acceleration (G): {vectorToString(args.entity.acceleration)}</p>
        }
      </div>
    </div>
  );
}

function Ship(args: { ship: Entity; index: number; setComputerShip: (ship: Entity | null) => void}) {
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
      <group ref={labelRef} position={scaleVector(args.ship.position, SCALE) as Vector3}>
        <mesh position={[0, 0, 0]} onPointerOver={()=> entityToShow.setEntityToShow(args.ship) }
        onPointerLeave={()=> entityToShow.setEntityToShow(null)
        } onClick={handleShipClick}>
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
            scaleVector(args.ship.acceleration, SCALE * TURN_IN_SECONDS * TURN_IN_SECONDS),
            scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)
          )}
          color="green"
        />
        <Text color="grey" fontSize={0.2} position={[0, -0.1, 0]} >
          {args.ship.name}
        </Text>
      </group>
    </>
  );
}

export function Ships(args: { setComputerShip: (ship: Entity | null) => void}) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.entities.ships.map((ship, index) => (
        <Ship key={ship.name} ship={ship} index={index} setComputerShip={args.setComputerShip} />
      ))}
    </>
  );
}

export function Missile(args: { missile: Entity; index: number}) {
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
      <group ref={labelRef} position={scaleVector(args.missile.position, SCALE) as Vector3}>
        <mesh position={[0, 0, 0]} onPointerOver={()=> entityToShow.setEntityToShow(args.missile) }
        onPointerLeave={()=> entityToShow.setEntityToShow(null)
        } >
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
            scaleVector(args.missile.acceleration, SCALE * TURN_IN_SECONDS * TURN_IN_SECONDS),
            scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)
          )}
          color="green"
        />
        <Text color="grey" fontSize={0.2} position={[0, -0.1, 0]} >
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
        <Missile key={missile.name} missile={missile} index={index}  />
      ))}
    </>
  )
}

export function Route(args: { plan: FlightPlan }) {
  console.log(`(Ships.Route) Display plan: ${JSON.stringify(args.plan)}`);
  console.log(`(Ships.Route) Display route: ${JSON.stringify(args.plan.path)}`);
  let start = scaleVector(args.plan.path[0], -1.0*SCALE);
  let prev = args.plan.path[0];
  let path = args.plan.path.slice(1);
  return (
    <group position={scaleVector(prev, SCALE) as Vector3}>
      {path.map((point, index) => {
        const oldPoint = prev;
        prev = point;
        return <Line key={index} start={addVector(start,scaleVector(oldPoint,SCALE))} end={addVector(start,scaleVector(point,SCALE))} color={"orange"} />;
      })}
    </group>
  );
}