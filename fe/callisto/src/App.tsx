import {useEffect, useState, useContext, useMemo} from "react";
import * as React from "react";
import * as THREE from "three";
import {Canvas, useThree} from "@react-three/fiber";
import {FlyControls} from "@react-three/drei";

import {Authentication} from "./Authentication";
import {ActionType} from "./Actions";
import SpaceView from "./Spaceview";
import {Ships, Missiles, Route} from "./Ships";
import {EntityInfoWindow, Controls, ViewControls} from "./Controls";
import {Effect, Explosions, ResultsWindow} from "./Effects";
import {setMessageHandlers, startWebsocket, computeFlightPath, resetServer, exit_scenario} from "./ServerManager";
import {Users, UserList} from "./UserList";

import {ShipComputer} from "./ShipComputer";
import {ActionsContextComponent, ActionContext} from "./Actions";
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
  ViewMode,
  ViewContextProvider,
} from "./Universal";

import {RoleChooser} from "./Role";
import {ScenarioManager} from "./ScenarioManager";

import "./index.css";

import {Tutorial} from "./Tutorial";

export const GOOGLE_OAUTH_CLIENT_ID: string =
  process.env.REACT_APP_GOOGLE_OAUTH_CLIENT_ID || "CannotFindClientId";

export function App() {
  const [authenticated, setAuthenticated] = useState<boolean>(false);
  const [email, setEmail] = useState<string | null>(null);
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
  const [actions, setActions] = useState<ActionType>({});
  const [users, setUsers] = useState<UserList>([]);
  const [scenarios, setScenarios] = useState<string[]>([]);
  const [scenarioTemplates, setScenarioTemplates] = useState<string[]>([]);
  const [joinedScenario, setJoinedScenario] = useState<string | null>(null);
  const [tutorialMode, setTutorialMode] = useState<boolean>(false);

  useEffect(() => {
    if (!socketReady) {
      setMessageHandlers(
        setEmail,
        setAuthenticated,
        setTemplates,
        setEntities,
        setActions,
        () => {},
        () => {},
        setUsers,
        (a, b) => { setScenarioTemplates(a); setScenarios(b);},
        (scenario: string) => setJoinedScenario(scenario),
      );

      startWebsocket(setSocketReady);
    }
  }, [socketReady]);

  useEffect(() => {
    if (!authenticated) {
      setJoinedScenario(null);
      setTutorialMode(false);
    }
  }, [authenticated]);

  return (
    <EntitiesServerProvider value={{entities: entities, handler: setEntities}}>
      <DesignTemplatesProvider value={{templates: templates, handler: setTemplates}}>
        <ActionsContextComponent actions={actions} setActions={setActions}>
          <div>
            {authenticated && socketReady && joinedScenario ? (
              <>
                <Simulator
                  tutorialMode={tutorialMode}
                  setAuthenticated={setAuthenticated}
                  email={email}
                  socketReady={socketReady}
                  setEmail={setEmail}
                  setJoinedScenario={setJoinedScenario}
                  users={users}
                  setUsers={setUsers}
                />
              </>
            ) : authenticated && socketReady ? (
              <ScenarioManager scenarios={scenarios} scenarioTemplates={scenarioTemplates} setTutorialMode={setTutorialMode} setAuthenticated={setAuthenticated} email={email} setEmail={setEmail} />
            ) : socketReady ? (
              <Authentication setAuthenticated={setAuthenticated} setEmail={setEmail} />
            ) : (
              <div>Waiting for socket to open...</div>
            )}
          </div>
        </ActionsContextComponent>
      </DesignTemplatesProvider>
    </EntitiesServerProvider>
  );
}

function Simulator({
  tutorialMode,
  setAuthenticated,
  email,
  setEmail,
  setJoinedScenario,
  socketReady,
  setUsers,
  users,
}: {
  tutorialMode: boolean;
  setAuthenticated: (authenticated: boolean) => void;
  email: string | null;
  setEmail: (email: string | null) => void;
  setJoinedScenario: (scenario: string | null) => void;
  socketReady: boolean;
  users: UserList;
  setUsers: (users: UserList) => void;
}) {
  const entitiesContext = useContext(EntitiesServerContext);
  const templatesContext = useContext(DesignTemplatesContext);
  const actionsContext = useContext(ActionContext);

  const [role, setRole] = useState<ViewMode>(ViewMode.General);
  const [shipName, setShipName] = useState<string | null>(null);

  const [entityToShow, setEntityToShow] = useState<Entity | null>(null);
  const [proposedPlan, setProposedPlan] = useState<FlightPathResult | null>(null);
  const [showResults, setShowResults] = useState<boolean>(false);
  const [events, setEvents] = useState<Effect[] | null>(null);
  const [cameraPos, setCameraPos] = useState<THREE.Vector3>(new THREE.Vector3(-100, 0, 0));
  const [camera, setCamera] = useState<THREE.Camera | null>(null);
  const [viewControls, setViewControls] = useState<ViewControlParams>({
    gravityWells: false,
    jumpDistance: false,
  });
  const [showRange, setShowRange] = useState<string | null>(null);
  const [stepIndex, setStepIndex] = useState(0);
  const [runTutorial, setRunTutorial] = useState<boolean>(true);
  const [computerShipName, setComputerShipName] = useState<string | null>(null);

  const computerShip = useMemo(() => {
    return entitiesContext.entities.ships.find((ship) => ship.name === computerShipName) || null;
  }, [entitiesContext.entities.ships, computerShipName]);

  useEffect(() => {
    if (socketReady) {
      setMessageHandlers(
        null,
        null,
        null,
        null,
        null,
        setProposedPlan,
        (effects: Effect[]) => {
          setEvents(effects);
          setShowResults(true);
        },
        null,
        null,
        null
      );
    }
  }, [
    socketReady,
    setAuthenticated,
    setEmail,
    templatesContext.handler,
    entitiesContext.handler,
    actionsContext.setActions,
    setUsers,
  ]);

  const getAndShowPlan = useMemo(
    () =>
      (
        entity_name: string | null,
        end_pos: [number, number, number],
        end_vel: [number, number, number],
        target_vel: [number, number, number] | null = null,
        target_accel: [number, number, number] | null = null,
        standoff: number = 0
      ) => {
        computeFlightPath(
          entity_name,
          end_pos,
          end_vel,
          setProposedPlan,
          target_vel,
          target_accel,
          standoff
        );
      },
    [setProposedPlan]
  );

  const [keysHeld, setKeyHeld] = useState({shift: false, slash: false});

  function downHandler({key}: {key: string}) {
    if (key === "Shift") {
      setKeyHeld({shift: true, slash: keysHeld.slash});
    }
  }

  function upHandler({key}: {key: string}) {
    if (key === "Shift") {
      setKeyHeld({shift: false, slash: keysHeld.slash});
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

  let tutorial_ship: Ship | null =
    entitiesContext.entities.ships.find((ship) => ship.name === "Killer") || null;
  if (tutorial_ship === undefined) {
    tutorial_ship = null;
  }

  return (
    <EntityToShowProvider
      value={{
        entityToShow: entityToShow,
        setEntityToShow: setEntityToShow,
      }}>
      <ViewContextProvider
        value={{
          role: role,
          setRole: (role) => setRole(role),
          shipName: shipName,
          setShipName: setShipName,
        }}>
        <>
          <div className="mainscreen-container">
            {!tutorialMode || (
              <Tutorial
                runTutorial={runTutorial}
                setRunTutorial={setRunTutorial}
                stepIndex={stepIndex}
                setStepIndex={setStepIndex}
                selectAShip={() => setComputerShipName(tutorial_ship?.name ?? null)}
                setAuthenticated={setAuthenticated}
              />
            )}
            {role !== ViewMode.Observer && (
              <Controls
                shipDesignTemplates={templatesContext.templates}
                computerShip={computerShip}
                setComputerShipName={setComputerShipName}
                getAndShowPlan={getAndShowPlan}
                setCameraPos={setCameraPos}
                camera={camera}
                setAuthenticated={setAuthenticated}
                showRange={showRange}
                setShowRange={setShowRange}
                proposedPlan={proposedPlan}
              />
            )}
            <div className="mainscreen-container">
              {[ViewMode.General, ViewMode.Pilot, ViewMode.Observer].includes(role) && (
                <ViewControls viewControls={viewControls} setViewControls={setViewControls} />
              )}
              <div className="admin-button-window">
                {!process.env.REACT_APP_RUN_TUTORIAL || (
                  <button
                    className="blue-button"
                    onClick={() => window.location.replace("https://callistoflight.com")}>
                    Exit Tutorial
                  </button>
                )}
                <Users users={users} email={email} />
                <RoleChooser />
                <div className="reset-and-logout-buttons">
                  <Exit setJoinedScenario={setJoinedScenario} email={email} />
                  {role === ViewMode.General && shipName == null && <button className="blue-button" onClick={resetServer}>Reset</button>}
                </div>
              </div>
              {role === ViewMode.General && computerShip && (
                <ShipComputer
                  ship={computerShip}
                  setComputerShipName={setComputerShipName}
                  proposedPlan={proposedPlan}
                  getAndShowPlan={getAndShowPlan}
                  sensorLocks={entitiesContext.entities.ships.reduce((acc, ship) => {
                    if (ship.sensor_locks.includes(computerShip.name)) {
                      acc.push(ship.name);
                    }
                    return acc;
                  }, [] as string[])}
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
                style={{position: "absolute"}}
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
                <Ships setComputerShipName={setComputerShipName} showRange={showRange} />
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
      </ViewContextProvider>
    </EntityToShowProvider>
  );
}

function GrabCamera(args: {
  cameraPos: THREE.Vector3;
  setCameraPos: (pos: THREE.Vector3) => void;
  setCamera: (camera: THREE.Camera) => void;
}) {
  const {camera} = useThree();
  useEffect(() => {
    args.setCameraPos(camera.position);
  });

  useEffect(() => {
    camera.position.set(args.cameraPos.x, args.cameraPos.y, args.cameraPos.z);
    args.setCamera(camera);
  });
  return null;
}

export function Exit(args: {
  setJoinedScenario: (scenario: string | null) => void;
  email: string | null;
}) {
  const logOut = () => {    
    args.setJoinedScenario(null);
    exit_scenario();
    console.log("(Authentication.Logout) Quit scenario");
  };

  const username = args.email ? args.email.split("@")[0] : "";
  return (
    <div className="logout-window">
      <button className="blue-button logout-button" onClick={logOut}>
        Exit {username}
      </button>
    </div>
  );
}
export default App;
