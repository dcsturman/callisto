import { useEffect, useState } from "react";
import { Canvas } from "@react-three/fiber";
import { FlyControls } from "@react-three/drei";
import SpaceView from "./Spaceview";
import { Ships, ShipInfoWindow, Missiles, Route } from "./Ships";
import { Effect, Effects } from "./Effects";

import {
  Entity,
  EntitiesServerProvider,
  EntityList,
  FlightPlan,
} from "./Contexts";

import Controls from "./Controls";
import "./index.css";
import {
  nextRound,
  addEntity,
  getEntities,
  setAcceleration,
  computeFlightPath,
} from "./ServerManager";

function App() {
  const [entities, setEntities] = useState<EntityList>({
    ships: [],
    planets: [],
    missiles: [],
  });
  const [shipToShow, setShipToShow] = useState<Entity | null>(null);
  const [computerShip, setComputerShip] = useState<Entity | null>(null);
  const [currentPlan, setCurrentPlan] = useState<FlightPlan | null>(null);
  const [events, setEvents] = useState<Effect[] | null>(null);

  useEffect(() => {
    getEntities(setEntities);
  }, []);

  let getAndShowPlan = (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number]
  ) => computeFlightPath(entity_name, end_pos, end_vel, setCurrentPlan);

  return (
    <div className="mainscreen-container">
      <>
        <EntitiesServerProvider
          value={{ entities: entities, handler: setEntities }}>
          <Controls
            nextRound={(callback) => nextRound(setEvents, callback)}
            addEntity={addEntity}
            setAcceleration={setAcceleration}
            computerShip={computerShip}
            setComputerShip={setComputerShip}
            currentPlan={currentPlan}
            getAndShowPlan={getAndShowPlan}
          />
          <Canvas
            camera={{
              fov: 75,
              near: 0.0001,
              far: 6000,
              position: [-100, 0, 0],
            }}>
            {/*<OrbitControls enableZoom={true} keys={keys} <ambientLight color={0xffffff} intensity={0.1} />/>*/}
            <FlyControls
              autoForward={false}
              dragToLook={true}
              movementSpeed={50}
              rollSpeed={0.5}
              makeDefault
            />
            <SpaceView />
            <Ships
              setShipToShow={setShipToShow}
              setComputerShip={setComputerShip}
            />
            <Missiles setShipToShow={setShipToShow}/>
            { events && events.length > 0&& <Effects effects={events} setEffects={setEvents} /> }
            {currentPlan && <Route plan={currentPlan} />}
          </Canvas>
        </EntitiesServerProvider>
      </>
      {shipToShow && <ShipInfoWindow ship={shipToShow} />}
    </div>
  );
}
export default App;
