import { useEffect, useState } from "react";
import { Canvas } from "@react-three/fiber";
import { OrbitControls, FlyControls } from "@react-three/drei";
import SpaceView from "./Spaceview";
import { Ships, ShipInfoWindow, Route } from "./Ships";

import { Entity, EntitiesServerProvider, FlightPlan } from "./Contexts";

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
  const [entities, setEntities] = useState<Entity[]>([]);
  const [shipToShow, setShipToShow] = useState<Entity | null>(null);
  const [computerShip, setComputerShip] = useState<Entity | null>(null);
  const [currentPlan, setCurrentPlan] = useState<FlightPlan | null>(null);

  const keys = { LEFT: "keyA", UP: "keyW", RIGHT: "keyD", BOTTOM: "keyS" };

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
        <EntitiesServerProvider value={entities}>
          <Controls
            nextRound={nextRound}
            getEntities={(entities) => setEntities(entities)}
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
              position: [-400, 0, 0],
            }}>
            {/*<OrbitControls enableZoom={true} keys={keys} <ambientLight color={0xffffff} intensity={0.1} />/>*/}
            <FlyControls 
              autoForward={false}
              dragToLook={true}
              movementSpeed={30}
              rollSpeed={0.5}
              makeDefault
            />
            
            <SpaceView />
            <Ships
              setShipToShow={setShipToShow}
              setComputerShip={setComputerShip}
            />
            {currentPlan && <Route plan={currentPlan} />}
          </Canvas>
        </EntitiesServerProvider>
      </>
      {shipToShow && <ShipInfoWindow ship={shipToShow} />}
    </div>
  );
}
export default App;
