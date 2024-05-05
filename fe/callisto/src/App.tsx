import { useEffect, useState } from "react";
import { Canvas } from "@react-three/fiber";
import { FlyControls, OrbitControls, TrackballControls } from "@react-three/drei";
import SpaceView from "./Spaceview";
import { Ships, ShipInfoWindow } from "./Ships";

import { Entity, EntitiesServerProvider } from "./Contexts";

import Controls from "./Controls";
import "./index.css";
import { nextRound, addEntity, getEntities, setAcceleration } from "./ServerManager";

function App() {
  const [entities, setEntities] = useState<Entity[]>([]);
  const [shipToShow, setShipToShow] = useState<Entity | null>(null);

  const keys = { LEFT: "keyA", UP: "keyW", RIGHT: "keyD", BOTTOM: "keyS" };

  useEffect(() => {
    getEntities(setEntities);
  }, []);
  
  return (
    <div className="mainscreen-container">
      <>
      <EntitiesServerProvider value={entities}>
        <Controls
          nextRound={nextRound}
          getEntities={(entities) => setEntities(entities) }
          addEntity={addEntity}
          setAcceleration={setAcceleration}
        />
        <Canvas
          camera={{
            fov: 75,
            near: 0.0001,
            far: 6000,
            position: [-400, 0, 0],
          }}
        >
          <OrbitControls enableZoom={true} keys={keys}/>
          {/* <FlyControls /> */}
          <ambientLight color={0xffffff} intensity={0.1} />
          <SpaceView />
          <Ships setShipToShow={setShipToShow}/>
        </Canvas>
      </EntitiesServerProvider>
      </>
      { shipToShow != null && <ShipInfoWindow ship={shipToShow} /> }
    </div>
  );
}
export default App;
