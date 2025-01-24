import { useEffect, useState } from "react";
import * as React from "react";
import * as THREE from "three";
import { Canvas, useThree } from "@react-three/fiber";
import { FlyControls } from "@react-three/drei";

import { Authentication, Logout } from "./Authentication";
import SpaceView from "./Spaceview";
import { Ships, Missiles, Route } from "./Ships";
import { EntityInfoWindow, Controls, ViewControls } from "./Controls";
import { Effect, Explosions, ResultsWindow } from "./Effects";
import {
  nextRound,
  getEntities,
  getTemplates,
  computeFlightPath,
} from "./ServerManager";

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
import { Tutorial } from "./Tutorial";

const POLL_ENTITIES_INTERVAL = 0;

export const GOOGLE_OAUTH_CLIENT_ID: string =
  process.env.REACT_APP_GOOGLE_OAUTH_CLIENT_ID || "CannotFindClientId";

export function App() {
  const [authenticated, setAuthenticated] = useState<boolean>(true);
  const [email, setEmail] = useState<string | null>(null);
  const [computerShipName, setComputerShipName] = useState<string | null>(null);

  console.group("Callisto Config parameters");
  if (process.env.REACT_APP_C_BACKEND) {
    console.log(
      "REACT_APP_C_BACKEND is set to: " + process.env.REACT_APP_C_BACKEND
    );
  } else {
    console.log("REACT_APP_C_BACKEND is not set.");
    console.log("ENV is set to: " + JSON.stringify(process.env));
  }

  if (process.env.REACT_APP_RUN_TUTORIAL) {
    console.log("Tutorial is set to run.");
  } else {
    console.log("Tutorial is not set to run.");
  }
  console.groupEnd();

  return (
    <div>
      {authenticated ? (
        <>
          <Simulator
            setAuthenticated={setAuthenticated}
            email={email}
            setEmail={setEmail}
            computerShipName={computerShipName}
            setComputerShipName={setComputerShipName}
          />
        </>
      ) : (
        <Authentication
          setAuthenticated={setAuthenticated}
          setEmail={setEmail}
        />
      )}
    </div>
  );
}

function Simulator({
  setAuthenticated,
  email,
  setEmail,
  computerShipName,
  setComputerShipName,
}: {
  setAuthenticated: (authenticated: boolean) => void;
  email: string | null;
  setEmail: (email: string | null) => void;
  computerShipName: string | null;
  setComputerShipName: (ship: string | null) => void;
}) {
  const [entities, setEntities] = useState<EntityList>({
    ships: [],
    planets: [],
    missiles: [],
  });
  const [entityToShow, setEntityToShow] = useState<Entity | null>(null);
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
    jumpDistance: false,
  });
  const [templates, setTemplates] = useState<ShipDesignTemplates>({});
  const [showRange, setShowRange] = useState<string | null>(null);
  const [stepIndex, setStepIndex] = useState(0);
  const [runTutorial, setRunTutorial] = useState<boolean>(true);

  useEffect(() => {
    if (POLL_ENTITIES_INTERVAL > 0) {
      const interval = setInterval(() => {
        getEntities(setEntities, setAuthenticated);
      }, POLL_ENTITIES_INTERVAL);
      return () => clearInterval(interval);
    }
  }, [entities, setAuthenticated]);

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
      setAuthenticated
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
    getTemplates(setTemplates, setAuthenticated);
    getEntities(setEntities, setAuthenticated);
  }, [setAuthenticated]);

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
            {!process.env.REACT_APP_RUN_TUTORIAL || (
              <Tutorial
                runTutorial={runTutorial}
                setRunTutorial={setRunTutorial}
                stepIndex={stepIndex}
                setStepIndex={setStepIndex}
                selectAShip={() => setComputerShipName("Killer")}
                setAuthenticated={setAuthenticated}
              />
            )}
            <Controls
              nextRound={(fireActions, callback) =>
                nextRound(
                  fireActions,
                  setEvents,
                  (es: EntityList) => {
                    setShowResults(true);
                    callback(es);
                  },
                  setAuthenticated
                )
              }
              computerShipName={computerShipName}
              setComputerShipName={setComputerShipName}
              shipDesignTemplates={templates}
              getAndShowPlan={getAndShowPlan}
              setCameraPos={setCameraPos}
              camera={camera}
              setAuthenticated={setAuthenticated}
              showRange={showRange}
              setShowRange={setShowRange}
            />
            <div className="mainscreen-container">
              <ViewControls
                viewControls={viewControls}
                setViewControls={setViewControls}
              />
              <div className="admin-button-window">
                {!process.env.REACT_APP_RUN_TUTORIAL || (
                  <button
                    className="blue-button"
                    onClick={() =>
                      window.location.replace("https://callistoflight.com")
                    }>
                    Exit Tutorial
                  </button>
                )}
                <Logout
                  setAuthenticated={setAuthenticated}
                  email={email}
                  setEmail={setEmail}
                />
              </div>
              {computerShipName && (
                <ShipComputer
                  shipName={computerShipName}
                  setComputerShipName={setComputerShipName}
                  proposedPlan={proposedPlan}
                  resetProposedPlan={resetProposedPlan}
                  getAndShowPlan={getAndShowPlan}
                  setAuthenticated={setAuthenticated}
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
                {/* eslint-disable react/no-unknown-property */ }
                <pointLight
                  position={[-148e3, 10, 10]}
                  intensity={6.0}
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
                <SpaceView
                  controlGravityWell={viewControls.gravityWells}
                  controlJumpDistance={viewControls.jumpDistance}
                />
                <Ships
                  setComputerShipName={setComputerShipName}
                  showRange={showRange}
                />
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
