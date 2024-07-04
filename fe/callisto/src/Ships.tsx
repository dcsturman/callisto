import { useContext, useRef } from "react";

import { Group } from "three";
import { Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "./Util";

import { Entity, EntitiesServerContext, FlightPlan } from "./Contexts";
import { Vector3 } from "@react-three/fiber";

import { SCALE, TIMEUNIT } from "./Contexts";
import { addVector, scaleVector, vectorToString } from "./Util";

export function ShipInfoWindow(args: { ship: Entity}) {
  return (
    <div className="ship-info-window">
      <h2 className="ship-info-title">{args.ship.name}</h2>
      <div className="ship-info-content">
        <p>Position (km): {vectorToString(scaleVector(args.ship.position,1e-3))}</p>
        <p>Velocity (m/s): {vectorToString(args.ship.velocity)}</p>
        <p>Acceleration (G): {vectorToString(args.ship.acceleration)}</p>
      </div>
    </div>
  );
}

function Ship(args: { ship: Entity; index: number; setShipToShow: (ship: Entity | null) => void; setComputerShip: (ship: Entity | null) => void}) {
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
        <mesh position={[0, 0, 0]} onPointerOver={()=> args.setShipToShow(args.ship) }
        onPointerLeave={()=> args.setShipToShow(null)
        } onClick={handleShipClick}>
          <sphereGeometry args={[0.1]} />
          <meshBasicMaterial color={[3, 3, 8.0]} />
        </mesh>
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.ship.velocity, SCALE * TIMEUNIT)}
          color="red"
        />
        <Line
          start={scaleVector(args.ship.velocity, SCALE * TIMEUNIT)}
          end={addVector(
            scaleVector(args.ship.acceleration, SCALE * TIMEUNIT * TIMEUNIT),
            scaleVector(args.ship.velocity, SCALE * TIMEUNIT)
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

export function Ships(args: { setShipToShow: (ship: Entity | null) => void; setComputerShip: (ship: Entity | null) => void}) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.entities.ships.map((ship, index) => (
        <Ship key={ship.name} ship={ship} index={index} setShipToShow={args.setShipToShow} setComputerShip={args.setComputerShip} />
      ))}
    </>
  );
}

export function Missile(args: { missile: Entity; index: number, setShipToShow: (ship: Entity | null) => void }) {
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
        <mesh position={[0, 0, 0]} onPointerOver={()=> args.setShipToShow(args.missile) }
        onPointerLeave={()=> args.setShipToShow(null)
        } >
          <sphereGeometry args={[0.05]} />
          <meshBasicMaterial color={[8.0, 0, 0]} />
        </mesh>
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.missile.velocity, SCALE * TIMEUNIT)}
          color="grey"
        />
        <Line
          start={scaleVector(args.missile.velocity, SCALE * TIMEUNIT)}
          end={addVector(
            scaleVector(args.missile.acceleration, SCALE * TIMEUNIT * TIMEUNIT),
            scaleVector(args.missile.velocity, SCALE * TIMEUNIT)
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

export function Missiles(args: { setShipToShow: (ship: Entity | null) => void }) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.entities.missiles.map((missile, index) => (
        <Missile key={missile.name} missile={missile} index={index} setShipToShow={args.setShipToShow} />
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
        console.log(`(Ships.Route) Displaying line from ${scaleVector(oldPoint,SCALE)} to ${scaleVector(point, SCALE)}`);
        return <Line key={index} start={addVector(start,scaleVector(oldPoint,SCALE))} end={addVector(start,scaleVector(point,SCALE))} color={"orange"} />;
      })}
    </group>
  );
}