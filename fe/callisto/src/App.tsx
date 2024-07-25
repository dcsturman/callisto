import { useEffect, useState } from "react";
import { Canvas } from "@react-three/fiber";
import { FlyControls } from "@react-three/drei";
import SpaceView from "./Spaceview";
import { Ships, EntityInfoWindow, Missiles, Route } from "./Ships";
import { ShipComputer } from "./Controls";
import { Effect, Effects } from "./Effects";

import {
  Entity,
  EntitiesServerProvider,
  EntityToShowProvider,
  EntityList,
  FlightPathResult,
  Ship,
} from "./Universal";

import Controls from "./Controls";
import "./index.css";
import {
  nextRound,
  getEntities,
  computeFlightPath,
} from "./ServerManager";

function App() {
  const [entities, setEntities] = useState<EntityList>({
    ships: [],
    planets: [],
    missiles: [],
  });
  const [entityToShow, setEntityToShow] = useState<Entity | null>(null);
  const [computerShip, setComputerShip] = useState<Ship | null>(null);
  const [currentPlan, setCurrentPlan] = useState<FlightPathResult | null>(null);
  const [events, setEvents] = useState<Effect[] | null>(null);

  useEffect(() => {
    getEntities(setEntities);
  }, []);

  let getAndShowPlan = (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null = null,
    standoff: number
  ) => computeFlightPath(entity_name, end_pos, end_vel, setCurrentPlan, target_vel, standoff);

  const [keysHeld, setKeyHeld] = useState({ shift: false, slash: false });

  function downHandler({ key }: { key: string }) {
    if (key === "Shift") {
      setKeyHeld({ shift: true, slash: keysHeld.slash });
    }
  }

  function upHandler({ key }: { key: string }) {
    if (key === "Shift") {
      setKeyHeld({ shift: false, slash: keysHeld.slash });
    }
  }

  useEffect(() => {
    window.addEventListener("keydown", downHandler);
    window.addEventListener("keyup", upHandler);
    return () => {
      window.removeEventListener("keydown", downHandler);
      window.removeEventListener("keyup", upHandler);
    };
  });

  return (
    <EntityToShowProvider
      value={{
        entityToShow: entityToShow,
        setEntityToShow: setEntityToShow,
      }}>
      <>
        <EntitiesServerProvider
          value={{ entities: entities, handler: setEntities }}>
          <ambientLight />
          <pointLight position={[-100, 10, 10]} />
          <div className="mainscreen-container">
            <Controls
              nextRound={(callback) => nextRound(setEvents, callback)}
              computerShip={computerShip}
              setComputerShip={setComputerShip}
              currentPlan={currentPlan}
              getAndShowPlan={getAndShowPlan}
            />
            <div className="mainscreen-container">
              {computerShip && (
                <ShipComputer
                  ship={computerShip}
                  setComputerShip={setComputerShip}
                  currentPlan={currentPlan}
                  getAndShowPlan={getAndShowPlan}
                />
              )}
              {/* Explicitly setting position to absolute seems to be necessary or it ends up relative and I cannot figure out why */}
              <Canvas
                style={{position: "absolute"}}
                className="spaceview-canvas"
                camera={{
                  fov: 75,
                  near: 0.0001,
                  far: 200000,
                  position: [-100, 0, 0],
                }}>
                <FlyControls
                  autoForward={false}
                  dragToLook={true}
                  movementSpeed={keysHeld.shift ? 1000 : 50}
                  rollSpeed={0.5}
                  makeDefault
                />
                <SpaceView />
                <Ships setComputerShip={setComputerShip} />
                <Missiles />
                {events && events.length > 0 && (
                  <Effects effects={events} setEffects={setEvents} />
                )}
                {currentPlan && <Route plan={currentPlan} />}
              </Canvas>
            </div>
          </div>
        </EntitiesServerProvider>
      </>
      {entityToShow && <EntityInfoWindow entity={entityToShow} />}
    </EntityToShowProvider>
  );
}
export default App;
