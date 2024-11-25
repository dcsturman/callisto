import { useEffect, useState } from "react";
import * as THREE from "three";
import { Canvas, useThree } from "@react-three/fiber";
import { FlyControls } from "@react-three/drei";

import { Authentication, Logout } from "./Authentication";
import SpaceView from "./Spaceview";
import { Ships, Missiles, Route } from "./Ships";
import {
  EntityInfoWindow,
  Controls,
  ViewControls,
} from "./Controls";
import { Effect, Explosions, ResultsWindow } from "./Effects";
import { nextRound, getEntities, getTemplates, computeFlightPath, CALLISTO_BACKEND } from "./ServerManager";

import { ShipComputer } from "./ShipComputer";

import {
  Entity,
  EntitiesServerProvider,
  EntityToShowProvider,
  EntityList,
  FlightPathResult,
  ShipDesignTemplates,
  ViewControlParams,
} from "./Universal";

import "./index.css";


function App() {
  const [authToken, setAuthToken] = useState<string | null>(null);
  const [email, setEmail] = useState<string | null>(null);
  if (process.env.REACT_APP_C_BACKEND) {
    console.log("REACT_APP_C_BACKEND is set to: " + process.env.REACT_APP_C_BACKEND);
  } else {
    console.log("REACT_APP_C_BACKEND is not set.");
    console.log("ENV is set to: " + JSON.stringify(process.env));
  }
  console.log(`Connecting to Callisto backend at ${CALLISTO_BACKEND}`);
  return (
    <div >
      {authToken ? (<>
        <Simulator token={authToken} setToken={setAuthToken} email={email} setEmail={setEmail} />
        </>
      ) : (
        <Authentication setAuthToken={setAuthToken} setEmail={setEmail} />
      )}
    </div>
  );
}

function Simulator(args: { token: string, setToken: (token: string | null) => void, email: string | null, setEmail: (email: string | null) => void }) {
  const [entities, setEntities] = useState<EntityList>({
    ships: [],
    planets: [],
    missiles: [],
  });
  const [entityToShow, setEntityToShow] = useState<Entity | null>(null);
  const [computerShipName, setComputerShipName] = useState<string | null>(null);
  const [proposedPlan, setProposedPlan] = useState<FlightPathResult | null>(
    null
  );
  const [showResults, setShowResults] = useState<boolean>(false);
  const [events, setEvents] = useState<Effect[] | null>(null);
  const [cameraPos, setCameraPos] = useState<THREE.Vector3>(
    new THREE.Vector3(-100, 0, 0)
  );
  const [camera, setCamera] = useState<THREE.Camera | null>(null);
  const [viewControls, setViewControls] = useState<ViewControlParams>({
    gravityWells: false,
    jumpDistance: false
  });
  const [templates, setTemplates] = useState<ShipDesignTemplates>({});
  const [showRange, setShowRange] = useState<string | null>(null);

  useEffect(() => {
    const interval = setInterval(() => {
      getEntities(setEntities, args.token, args.setToken);
    }, 2000);
    return () => clearInterval(interval);
  }, [entities, args.token, args.setToken]);

  const getAndShowPlan = (
    entity_name: string | null,
    end_pos: [number, number, number],
    end_vel: [number, number, number],
    target_vel: [number, number, number] | null = null,
    standoff: number = 0
  ) => {
    computeFlightPath(
      entity_name,
      end_pos,
      end_vel,
      setProposedPlan,
      target_vel,
      standoff,
      args.token,
      args.setToken
    );
  };

  const resetProposedPlan = () => {
    setProposedPlan(null);
  };

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
    getTemplates(setTemplates, args.token, args.setToken);
    getEntities(setEntities, args.token, args.setToken);
  }, [args.token, args.setToken]);

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
          <div className="mainscreen-container">
            <Controls
              nextRound={(fireActions, callback) =>
                nextRound(fireActions, setEvents, (es: EntityList) => {
                  setShowResults(true);
                  callback(es);
                }, args.token, args.setToken)
              }
              computerShipName={computerShipName}
              setComputerShipName={setComputerShipName}
              shipDesignTemplates={templates}
              getAndShowPlan={getAndShowPlan}
              setCameraPos={setCameraPos}
              camera={camera}
              token={args.token}
              setToken={args.setToken}
              showRange={showRange}
              setShowRange={setShowRange}
            />
            <div className="mainscreen-container">
              <ViewControls
                viewControls={viewControls}
                setViewControls={setViewControls}
              />
              <Logout
                setAuthToken={args.setToken}
                email={args.email}
                setEmail={args.setEmail}
              />
              {computerShipName && (
                <ShipComputer
                  shipName={computerShipName}
                  setComputerShipName={setComputerShipName}
                  proposedPlan={proposedPlan}
                  resetProposedPlan={resetProposedPlan}
                  getAndShowPlan={getAndShowPlan}
                  token={args.token}
                  setToken={args.setToken}
                />
              )}
              {showResults && (
                <ResultsWindow
                  clearShowResults={() => setShowResults(false)}
                  effects={events}
                  setEffects={setEvents}
                />
              )}
              <Canvas
                style={{ position: "absolute" }}
                className="spaceview-canvas"
                camera={{
                  fov: 75,
                  near: 0.0001,
                  far: 200000,
                  position: [-100, 0, 0],
                }}>
                <pointLight
                  position={[-148e3, 10, 10]}
                  intensity={6}
                  decay={0.01}
                  color="#fff7cd"
                />
                <ambientLight intensity={1.0} />
                <FlyControls
                  autoForward={false}
                  dragToLook={true}
                  movementSpeed={keysHeld.shift ? 1000 : 50}
                  rollSpeed={0.5}
                  makeDefault
                />
                <GrabCamera
                  cameraPos={cameraPos}
                  setCameraPos={setCameraPos}
                  setCamera={setCamera}
                />
                <SpaceView controlGravityWell={viewControls.gravityWells} controlJumpDistance={viewControls.jumpDistance} />
                <Ships setComputerShipName={setComputerShipName} showRange={showRange} />
                <Missiles />
                {events && events.length > 0 && (
                  <Explosions effects={events} setEffects={setEvents} />
                )}
                {proposedPlan && <Route plan={proposedPlan} />}
              </Canvas>
            </div>
          </div>
        </EntitiesServerProvider>
      </>
      {entityToShow && <EntityInfoWindow entity={entityToShow} />}
    </EntityToShowProvider>
  );
}

function GrabCamera(args: {
  cameraPos: THREE.Vector3;
  setCameraPos: (pos: THREE.Vector3) => void;
  setCamera: (camera: THREE.Camera) => void;
}) {
  const { camera } = useThree();
  useEffect(() => {
    args.setCameraPos(camera.position);
  });

  useEffect(() => {
    camera.position.set(args.cameraPos.x, args.cameraPos.y, args.cameraPos.z);
    args.setCamera(camera);
  });
  return null;
}

export default App;
