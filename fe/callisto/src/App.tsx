import { useEffect, useState, useContext } from "react";
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
  setMessageHandlers,
  startWebsocket,
  nextRound,
  computeFlightPath,
} from "./ServerManager";
import { Users, UserList } from "./UserList";

import { ShipComputer } from "./ShipComputer";

import {
  Entity,
  EntitiesServerProvider,
  EntityToShowProvider,
  EntityList,
  FlightPathResult,
  Ship,
  ShipDesignTemplates,
  ViewControlParams,
  DesignTemplatesContext,
  EntitiesServerContext,
  DesignTemplatesProvider,
} from "./Universal";

import "./index.css";
import { Tutorial } from "./Tutorial";

export const GOOGLE_OAUTH_CLIENT_ID: string =
  process.env.REACT_APP_GOOGLE_OAUTH_CLIENT_ID || "CannotFindClientId";

export function App() {
  const [authenticated, setAuthenticated] = useState<boolean>(false);
  const [email, setEmail] = useState<string | null>(null);
  const [computerShip, setComputerShip] = useState<Ship | null>(null);
  const [socketReady, setSocketReady] = useState<boolean>(false);

  // Logically entities and templates make more sense in the Simulator. However,
  // we need to set the handlers at this level in case we just reauthenticate successfully
  // (e.g. on screen refresh when the cookie is valid).
  const [entities, setEntities] = useState<EntityList>({
    ships: [],
    planets: [],
    missiles: [],
  });
  
  const [templates, setTemplates] = useState<ShipDesignTemplates>({});
  const [users, setUsers] = useState<UserList>([] as unknown as UserList);

  console.groupCollapsed("Callisto Config parameters");
  if (process.env.REACT_APP_CALLISTO_BACKEND) {
    console.log(
      "REACT_APP_CALLISTO_BACKEND is set to: " + process.env.REACT_APP_CALLISTO_BACKEND
    );
  } else {
    console.log("REACT_APP_CALLISTO_BACKEND is not set.");
    console.log("ENV is set to: " + JSON.stringify(process.env));
  }

  console.log("Running on " + window.location.href);
  if (process.env.REACT_APP_RUN_TUTORIAL) {
    console.log("Tutorial is set to run.");
  } else {
    console.log("Tutorial is not set to run.");
  }
  console.groupEnd();

  useEffect(() => {
    setMessageHandlers(
      setEmail,
      setAuthenticated,
      setTemplates,
      setEntities,
      () => {},
      () => {},
      setUsers,
    );
    if (!socketReady) {
      startWebsocket(setSocketReady);
    } 
  }, [socketReady]);

  return (
    <EntitiesServerProvider value={{ entities: entities, handler: setEntities }}>
    <DesignTemplatesProvider value={{templates: templates, handler: setTemplates}}>
    <div>
      {authenticated && socketReady ? (
        <>
          <Simulator
            setAuthenticated={setAuthenticated}
            email={email}
            socketReady={socketReady}
            setEmail={setEmail}
            computerShip={computerShip}
            setComputerShip={setComputerShip}
            users={users}
            setUsers={setUsers}
          />
        </>
      ) : socketReady ? (
        <Authentication
          setAuthenticated={setAuthenticated}
          setEmail={setEmail}
        />
      ) : (
        <div>Waiting for socket to open...</div>
      )}
    </div>
    </DesignTemplatesProvider>
    </EntitiesServerProvider>
  );
}

function Simulator({
  setAuthenticated,
  email,
  setEmail,
  socketReady,
  computerShip,
  setComputerShip,
  setUsers,
  users
}: {
    setAuthenticated: (authenticated: boolean) => void;
  email: string | null;
  setEmail: (email: string | null) => void;
  socketReady: boolean;
  computerShip: Ship | null;
  setComputerShip: (ship: Ship | null) => void;
  users: UserList;
  setUsers: (users: UserList) => void;
}) {

  const entitiesContext = useContext(EntitiesServerContext);
  const templatesContext = useContext(DesignTemplatesContext);

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
  const [showRange, setShowRange] = useState<string | null>(null);
  const [stepIndex, setStepIndex] = useState(0);
  const [runTutorial, setRunTutorial] = useState<boolean>(true);

  useEffect(() => {
    if (socketReady) {
      setMessageHandlers(
        setEmail,
        setAuthenticated,
        templatesContext.handler,
        entitiesContext.handler,
        setProposedPlan,
        (effects: Effect[]) => {
          setEvents(effects);
          setShowResults(true);
        },
        setUsers,
      );
    }
  }, [socketReady, setAuthenticated, setEmail,templatesContext.handler, entitiesContext.handler, setUsers]);

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
      standoff
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
    window.addEventListener("keydown", downHandler);
    window.addEventListener("keyup", upHandler);
    return () => {
      window.removeEventListener("keydown", downHandler);
      window.removeEventListener("keyup", upHandler);
    };
  });

  let tutorial_ship: Ship | null = entitiesContext.entities.ships.find((ship) => ship.name === "Killer") || null;
  if (tutorial_ship === undefined) {
    tutorial_ship = null;
  }

  return (
    <EntityToShowProvider
      value={{
        entityToShow: entityToShow,
        setEntityToShow: setEntityToShow,
      }}>
      <>
          <div className="mainscreen-container">
            {!process.env.REACT_APP_RUN_TUTORIAL || (
              <Tutorial
                runTutorial={runTutorial}
                setRunTutorial={setRunTutorial}
                stepIndex={stepIndex}
                setStepIndex={setStepIndex}
                selectAShip={() => setComputerShip(tutorial_ship)}
                setAuthenticated={setAuthenticated}
              />
            )}
            <Controls
              nextRound={(fireActions) => nextRound(fireActions)}
              shipDesignTemplates={templatesContext.templates}
              computerShip={computerShip}
              setComputerShip={setComputerShip}
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
                <Users users={users} email={email}/>
                <Logout
                  setAuthenticated={setAuthenticated}
                  email={email}
                  setEmail={setEmail}
                />
              </div>
              {computerShip && (
                <ShipComputer
                  ship={computerShip}
                  setComputerShip={setComputerShip}
                  proposedPlan={proposedPlan}
                  resetProposedPlan={resetProposedPlan}
                  getAndShowPlan={getAndShowPlan}
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
                {/* eslint-disable react/no-unknown-property */}
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
                  setComputerShip={setComputerShip}
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
