import { useEffect, useState, useMemo, lazy, Suspense } from "react";
import * as React from "react";
import * as THREE from "three";
import { Canvas, useThree } from "@react-three/fiber";
import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { KernelSize } from "postprocessing";
import { FlyControls } from "./lib/FlyControls";

import { Authentication } from "components/scenarios/Authentication";

// Lazy load 3D components to reduce initial bundle size
const SpaceView = lazy(() => import("components/space/Spaceview"));
const Ships = lazy(() =>
  import("./components/space/Ships").then((m) => ({ default: m.Ships })),
);
const Missiles = lazy(() =>
  import("./components/space/Ships").then((m) => ({ default: m.Missiles })),
);
const Route = lazy(() =>
  import("./components/space/Ships").then((m) => ({ default: m.Route })),
);
const Explosions = lazy(() =>
  import("./components/space/Effects").then((m) => ({ default: m.Explosions })),
);
const ResultsWindow = lazy(() =>
  import("./components/space/Effects").then((m) => ({
    default: m.ResultsWindow,
  })),
);

import {
  EntityInfoWindow,
  Controls,
  ViewControls,
} from "./components/controls/Controls";
import { ShipSummary } from "./components/controls/ShipSummary";
import {
  startWebsocket,
  resetServer,
  exit_scenario,
  setUpKeepAlive,
  socket,
} from "lib/serverManager";
import { Users } from "components/UserList";

import { ShipComputer } from "components/controls/ShipComputer";
import { ViewMode } from "lib/view";

import { RoleChooser } from "components/Role";
import {
  ScenarioManager,
  TUTORIAL_PREFIX,
} from "components/scenarios/ScenarioManager";
import { Tutorial } from "components/Tutorial";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { AppMode, setAppMode } from "state/tutorialSlice";
import { setJoinedScenario, setRoleShip } from "state/userSlice";
import { entitiesSelector } from "state/serverSlice";

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
      dispatch(setAppMode(AppMode.Game));
    }
  }, [authenticated, dispatch]);

  useEffect(() => {
    if (!joinedScenario) {
      dispatch(setRoleShip([ViewMode.General, null]));
      dispatch(setAppMode(AppMode.Game));
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
        <div>Waiting for socket to open...</div>
      )}
    </div>
  );
}

function Simulator() {
  const entities = useAppSelector(entitiesSelector);
  const users = useAppSelector((state) => state.server.users);
  const appMode = useAppSelector((state) => state.tutorial.appMode);
  const tutorialMode = appMode === AppMode.Tutorial;
  const scenarioBuilderMode = appMode === AppMode.ScenarioBuilder;

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
        cameraQuaternion[3],
      );
    }
  }, [camera, cameraPos, cameraQuaternion]);

  // const [stepIndex, setStepIndex] = useState(0);
  // const [runTutorial, setRunTutorial] = useState<boolean>(true);

  const computerShip = useMemo(() => {
    return (
      entities.ships.find((ship) => ship.name === computerShipName) || null
    );
  }, [entities.ships, computerShipName]);

  return (
    <>
      <div className="mainscreen-container">
        {!tutorialMode || <Tutorial />}
        {(scenarioBuilderMode || role !== ViewMode.Observer) && <Controls />}
        {(scenarioBuilderMode ||
          [ViewMode.General, ViewMode.Pilot, ViewMode.Observer].includes(
            role,
          )) && (
          <div className="top-right-stack">
            <ShipSummary />
            <ViewControls />
          </div>
        )}
        <div className="admin-button-window">
          <h2>
            {joinedScenario &&
              (tutorialMode
                ? "Tutorial"
                : scenarioBuilderMode
                  ? `Scenario Builder`
                  : joinedScenario)}
          </h2>
          <Users users={users} email={email} />
          {!scenarioBuilderMode && <RoleChooser />}
          <div className="reset-and-logout-buttons">
            <Exit email={email} />
            {role === ViewMode.General && shipName == null && (
              <button
                className="blue-button"
                onClick={() => resetServer(appMode)}
              >
                Reset
              </button>
            )}
          </div>
        </div>
        {!scenarioBuilderMode && role === ViewMode.General && computerShip && (
          <ShipComputer ship={computerShip} />
        )}
        {showResults && (
          <Suspense fallback={null}>
            <ResultsWindow />
          </Suspense>
        )}
        <Canvas
          style={{ position: "absolute" }}
          id="main-canvas"
          className="spaceview-canvas"
          camera={{
            fov: 75,
            near: 0.0001,
            far: 200000,
            position: cameraPos,
            quaternion: cameraQuaternion,
          }}
        >
          {/* eslint-disable react/no-unknown-property */}
          <pointLight
            position={[-148e3, 10, 10]}
            intensity={6.0}
            decay={0.01}
            color="#fff7cd"
          />
          {/* Low: ambient multiplies a texture at full strength with no
              shading falloff, so at 1.0 Jupiter's near-white cloud bands
              clipped to featureless blocks — which then bloomed as one huge
              bright area. */}
          <ambientLight intensity={0.3} />
          <GrabCamera setCamera={setCamera} />
          <FlyControls
            containerName="main-canvas"
            camera={camera!}
            autoForward={false}
            dragToLook={true}
            movementSpeed={50}
            rollSpeed={0.2}
            enableFlywheelZoom={true}
          />
          <Suspense fallback={null}>
            <SpaceView />
            <Ships />
            <Missiles />
            {events && events.length > 0 && <Explosions />}
            {proposedPlan && <Route plan={proposedPlan} />}
            {/* One composer for the whole scene.  Bloom is a full-screen
                pass over the finished frame, so a composer per entity never
                glowed "its" entity — each simply re-rendered the whole scene
                and re-bloomed the whole frame.  Fourteen of them on a busy
                scenario, for one frame's worth of picture. */}
            <EffectComposer>
              {/* Do NOT re-add mipmapBlur.  It silently produces nothing on
                  three 0.182 — postprocessing 6.38 supports only "< 0.183.0"
                  and that is its newest code path.  No error, no warning, just
                  a pass that outputs zero, which is what made the ships flat
                  white discs.  The classic kernel blur below works.  If three
                  or postprocessing is upgraded, mipmapBlur is worth retrying:
                  it is cheaper than a HUGE kernel. */}
              {/* The threshold is high enough that lit planet surfaces stay
                  out of it, while the ships — HDR well above 1.0 — still
                  bloom.  Tuned against jupiter.json, where a planet filling
                  the view is the worst case for an area-driven effect. */}
              <Bloom
                kernelSize={KernelSize.HUGE}
                luminanceThreshold={0.9}
                luminanceSmoothing={0.05}
                intensity={4.0}
              />
            </EffectComposer>
          </Suspense>
        </Canvas>
      </div>
      {entityToShow && <EntityInfoWindow entity={entityToShow} />}
    </>
  );
}

function GrabCamera(args: { setCamera: (camera: THREE.Camera) => void }) {
  const { camera } = useThree();
  useEffect(() => {
    args.setCamera(camera);
  }, [camera, args, args.setCamera]);

  return null;
}

export function Exit(args: { email: string | null }) {
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
