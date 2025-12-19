import {useEffect, useState, useMemo} from "react";
import * as React from "react";
import * as THREE from "three";
import {Canvas, useThree} from "@react-three/fiber";
import {FlyControls} from "./lib/FlyControls";

import {Authentication} from "components/scenarios/Authentication";
import SpaceView from "components/space/Spaceview";
import {Ships, Missiles, Route} from "./components/space/Ships";
import {EntityInfoWindow, Controls, ViewControls} from "./components/controls/Controls";
import {Explosions, ResultsWindow} from "./components/space/Effects";
import {
  startWebsocket,
  resetServer,
  exit_scenario,
  setUpKeepAlive,
  socket,
} from "lib/serverManager";
import {Users} from "components/UserList";

import {ShipComputer} from "components/controls/ShipComputer";
import {ViewMode} from "lib/view";

import {RoleChooser} from "components/Role";
import {ScenarioManager, TUTORIAL_PREFIX} from "components/scenarios/ScenarioManager";
import {Tutorial} from "components/Tutorial";

import {useAppSelector, useAppDispatch} from "state/hooks";
import {setTutorialMode} from "state/tutorialSlice";
import {setJoinedScenario, setRoleShip} from "state/userSlice";
import {entitiesSelector} from "state/serverSlice";

import "./index.css";

export const GOOGLE_OAUTH_CLIENT_ID: string =
  import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID || "CannotFindClientId";

export function App() {
  const socketReady = useAppSelector((state) => state.server.socketReady);
  const authenticated = useAppSelector((state) => state.server.authenticated);
  const joinedScenario = useAppSelector((state) => state.user.joinedScenario);

  const dispatch = useAppDispatch();

  useEffect(() => {
    if (!socketReady || !socket) {
      startWebsocket();
      setUpKeepAlive();
    }
  }, [socketReady]);

  useEffect(() => {
    if (!authenticated) {
      dispatch(setJoinedScenario(null));
      dispatch(setTutorialMode(false));
    }
  }, [authenticated, dispatch]);

  useEffect(() => {
    if (!joinedScenario) {
      dispatch(setRoleShip([ViewMode.General, null]));
      dispatch(setTutorialMode(false));
    }
  }, [joinedScenario, dispatch]);

  console.log("Authenticated: " + authenticated.toString());
  return (
    <div>
      {authenticated && socketReady && joinedScenario ? (
        <>
          <Simulator />
        </>
      ) : authenticated && socketReady ? (
        <ScenarioManager />
      ) : socketReady ? (
        <Authentication />
      ) : (
        <div>
          Waiting for socket to open...
        </div>
      )}
    </div>
  );
}

function Simulator() {
  const entities = useAppSelector(entitiesSelector);
  const users = useAppSelector((state) => state.server.users);
  const tutorialMode = useAppSelector((state) => state.tutorial.tutorialMode);

  const role = useAppSelector((state) => state.user.role);
  const shipName = useAppSelector((state) => state.user.shipName);
  const joinedScenario = useAppSelector((state) => state.user.joinedScenario);
  const email = useAppSelector((state) => state.user.email);

  const entityToShow = useAppSelector((state) => state.ui.entityToShow);
  const proposedPlan = useAppSelector((state) => state.ui.proposedPlan);
  const showResults = useAppSelector((state) => state.ui.showResults);
  const events = useAppSelector((state) => state.ui.events);
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const cameraPos = useAppSelector((state) => state.ui.cameraPos);
  const cameraQuaternion = useAppSelector((state) => state.ui.cameraQuaternion);

  const [camera, setCamera] = useState<THREE.Camera | null>(null);

  useEffect(() => {
    if (camera) {
      camera.position.set(cameraPos[0], cameraPos[1], cameraPos[2]);
      camera.quaternion.set(
        cameraQuaternion[0],
        cameraQuaternion[1],
        cameraQuaternion[2],
        cameraQuaternion[3]
      );
    }
  }, [camera, cameraPos, cameraQuaternion]);

  // const [stepIndex, setStepIndex] = useState(0);
  // const [runTutorial, setRunTutorial] = useState<boolean>(true);

  const computerShip = useMemo(() => {
    return entities.ships.find((ship) => ship.name === computerShipName) || null;
  }, [entities.ships, computerShipName]);

  return (
    <>
      <div className="mainscreen-container">
        {!tutorialMode || <Tutorial />}
        {role !== ViewMode.Observer && <Controls />}
        {[ViewMode.General, ViewMode.Pilot, ViewMode.Observer].includes(role) && <ViewControls />}
        <div className="admin-button-window">
          <h2>
            {joinedScenario &&
              (joinedScenario.startsWith(TUTORIAL_PREFIX) ? "Tutorial" : joinedScenario)}
          </h2>
          <Users users={users} email={email} />
          <RoleChooser />
          <div className="reset-and-logout-buttons">
            <Exit email={email} />
            {role === ViewMode.General && shipName == null && (
              <button
                className="blue-button"
                onClick={() => resetServer(joinedScenario?.startsWith(TUTORIAL_PREFIX) ?? false)}>
                Reset
              </button>
            )}
          </div>
        </div>
        {role === ViewMode.General && computerShip && <ShipComputer ship={computerShip} />}
        {showResults && <ResultsWindow />}
        <Canvas
          style={{position: "absolute"}}
          id="main-canvas"
          className="spaceview-canvas"
          camera={{
            fov: 75,
            near: 0.0001,
            far: 200000,
            position: cameraPos,
            quaternion: cameraQuaternion,
          }}>
          {/* eslint-disable react/no-unknown-property */}
          <pointLight position={[-148e3, 10, 10]} intensity={6.0} decay={0.01} color="#fff7cd" />
          <ambientLight intensity={1.0} />
          <GrabCamera setCamera={setCamera} />
          <FlyControls
            containerName="main-canvas"
            camera={camera!}
            autoForward={false}
            dragToLook={true}
            movementSpeed={50}
            rollSpeed={0.2}
          />
          <SpaceView />
          <Ships />
          <Missiles />
          {events && events.length > 0 && <Explosions />}
          {proposedPlan && <Route plan={proposedPlan} />}
        </Canvas>
      </div>
      {entityToShow && <EntityInfoWindow entity={entityToShow} />}
    </>
  );
}

function GrabCamera(args: {setCamera: (camera: THREE.Camera) => void}) {
  const {camera} = useThree();
  useEffect(() => {
    args.setCamera(camera);
  }, [camera, args, args.setCamera]);

  return null;
}

export function Exit(args: {email: string | null}) {
  const dispatch = useAppDispatch();
  const exit = () => {
    dispatch(setJoinedScenario(null));
    exit_scenario();
    console.log("(Authentication.Logout) Quit scenario");
  };

  const username = args.email ? args.email.split("@")[0] : "";
  return (
    <div className="logout-window">
      <button className="blue-button logout-button" onClick={exit}>
        Exit {username}
      </button>
    </div>
  );
}
export default App;
