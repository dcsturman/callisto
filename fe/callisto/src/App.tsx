import { useEffect, useState, useMemo, lazy, Suspense } from "react";
import * as React from "react";
import * as THREE from "three";
import { Canvas, useThree } from "@react-three/fiber";
import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { KernelSize } from "postprocessing";
import { FlyControls } from "./lib/FlyControls";
import { FleetComms } from "components/controls/FleetComms";
import { OrdersPanel } from "components/controls/OrdersPanel";

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

import { Stations } from "components/controls/Stations";
import { ViewMode, hasRole, isReferee } from "lib/view";

import { RoleChooser } from "components/Role";
import { ScenarioManager, TUTORIAL_BUILDER_PREFIX } from "components/scenarios/ScenarioManager";
import { Coach } from "components/tutorial/Coach";
import { VictoryScreen } from "components/VictoryScreen";
import { ConfirmDialog } from "components/ConfirmDialog";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { AppMode, setAppMode } from "state/tutorialSlice";
import { setJoinedScenario, setRoleShip } from "state/userSlice";
import { entitiesSelector } from "state/serverSlice";

import "./index.css";

export const GOOGLE_OAUTH_CLIENT_ID: string =
  import.meta.env.VITE_GOOGLE_OAUTH_CLIENT_ID || "CannotFindClientId";

/**
 * How fast the camera turns under the arrow and roll keys.
 *
 * Was 0.2, which worked out to about 23 degrees a second -- four seconds to
 * bring the view ninety degrees around, which is why turning felt broken. 0.6
 * is a shade under 70 degrees a second, and holding Shift trebles it.
 */
const CAMERA_ROLL_SPEED = 0.6;

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
      dispatch(setRoleShip([[ViewMode.General], null]));
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

  const roles = useAppSelector((state) => state.user.roles);
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

  // The panels down the right-hand side stop above the session panel in the
  // bottom corner, whose height changes with its user list: on a short screen
  // they otherwise ran down behind it and the two drew over each other.
  const adminWindow = React.useRef<HTMLDivElement>(null);

  // The session panel folds to one line: the game and who is in it. Folded to
  // start with on a laptop-sized screen; the player's choice is remembered.
  const [sessionOpen, setSessionOpen] = useState(() => {
    try {
      const saved = window.localStorage.getItem("session-panel-open");
      if (saved != null) return saved === "1";
    } catch {
      // No storage: fall through to the default.
    }
    return window.innerWidth >= 1600;
  });
  const toggleSession = () => {
    setSessionOpen((open) => {
      try {
        window.localStorage.setItem("session-panel-open", open ? "0" : "1");
      } catch {
        // Not remembered.
      }
      return !open;
    });
  };

  // Tab hides every panel, to see the battle, and Tab brings them back. Not
  // while typing or choosing from a menu, where Tab means the next field;
  // Shift+Tab is left alone for anyone moving through the controls by key.
  const [panelsHidden, setPanelsHidden] = useState(false);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Tab" || event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) return;
      const focus = document.activeElement as HTMLElement | null;
      const typing =
        focus != null &&
        (focus.tagName === "INPUT" ||
          focus.tagName === "TEXTAREA" ||
          focus.tagName === "SELECT" ||
          focus.isContentEditable);
      if (typing) return;
      event.preventDefault();
      setPanelsHidden((hidden) => !hidden);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // How many cards the console holds, for its CSS: it uses no more columns
  // than it has cards. Counted from the DOM, since the station cards come and
  // go with the roles and ship, which this component does not re-render for.
  const consoleRef = React.useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = consoleRef.current;
    if (element == null) return undefined;
    const count = () => element.style.setProperty("--console-cards", String(Math.max(1, element.children.length)));
    count();
    const observer = new MutationObserver(count);
    observer.observe(element, { childList: true });
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const element = adminWindow.current;
    if (element == null || typeof ResizeObserver === "undefined") {
      return;
    }
    const root = document.documentElement;
    const observer = new ResizeObserver(() =>
      root.style.setProperty("--admin-window-height", `${element.offsetHeight}px`),
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      root.style.removeProperty("--admin-window-height");
    };
  }, []);

  return (
    <>
      <div className={panelsHidden ? "mainscreen-container panels-hidden" : "mainscreen-container"}>
        {panelsHidden && (
          <div className="panels-hidden-note" role="status">
            Panels hidden. Press <kbd>Tab</kbd> to show them.
          </div>
        )}
        {/* A scenario being built has no winner to announce. A lesson does:
            seeing the real screen teaches it better than describing it. */}
        {joinedScenario && !scenarioBuilderMode && <VictoryScreen />}
        {/* The console: the ship's dossier, then a card for every station
            this player works, in one column that spills into a second only
            when the first is full. Screen space is the scarce thing on a
            laptop, and the old layout reserved a column whether it had
            anything to put in it or not. */}
        {/* The scroll box, and inside it the columns. Kept apart because a
            multi-column box with a height limit sends what does not fit
            into more columns off to the side instead of scrolling: a card
            taller than the screen was cut off. */}
        <div className="console">
          <div className="console-columns" ref={consoleRef}>
            {(scenarioBuilderMode || !hasRole(roles, ViewMode.Observer)) && <Controls />}
            {!scenarioBuilderMode && !hasRole(roles, ViewMode.Observer) && computerShip && (
              <Stations ship={computerShip} />
            )}
          </div>
        </div>
        {/* The ship list and view options are for everyone at the table. They
            were limited to General, Pilot and Observer, which left an engineer
            or a gunner with no way to see the other ships or turn the range
            rings on. */}
        <div className="top-right-stack">
          {/* The tutorial's coach, first in the column, where it is read. */}
          {(tutorialMode || joinedScenario?.startsWith(TUTORIAL_BUILDER_PREFIX)) && <Coach />}
          {/* View and Ships side by side across the top, so neither pushes
              the other down; each folds to its header. */}
          <div className="top-right-row">
            <ViewControls />
            <ShipSummary />
          </div>
          {/* Results belong with the other reference panels rather than
              floating beside the console, where their width moved them
              every time the console gained or lost a column. */}
          {showResults && (
            <Suspense fallback={null}>
              <ResultsWindow />
            </Suspense>
          )}
          {/* Orders in words, for the referee and a fleet or squadron
              commander to give: an AI fleet commander reads them. */}
          {joinedScenario && !scenarioBuilderMode && <OrdersPanel />}
          {/* What the AI captains are saying: the referee hears every AI
              side without opening each ship. Nothing when no ship is AI. */}
          {joinedScenario && !scenarioBuilderMode && <FleetComms />}
        </div>
        <div className="admin-button-window" ref={adminWindow} data-tutorial="session-panel">
          <h2>
            <button
              type="button"
              className="session-toggle"
              aria-expanded={sessionOpen}
              title={sessionOpen ? "Fold the session panel" : "Show who is playing, your seat, and Exit"}
              onClick={toggleSession}>
              {sessionOpen ? "\u25BE" : "\u25B8"}{" "}
              {joinedScenario &&
                (tutorialMode
                  ? "Tutorial"
                  : scenarioBuilderMode
                    ? `Scenario Builder`
                    : joinedScenario)}
              {!sessionOpen && (
                <span className="session-count">
                  {" "}
                  · {users.length} {users.length === 1 ? "player" : "players"}
                </span>
              )}
            </button>
          </h2>
          {sessionOpen && (
            <>
              <Users users={users} email={email} ships={entities.ships} />
              {!scenarioBuilderMode && <RoleChooser />}
              <div className="reset-and-logout-buttons">
                <Exit email={email} />
                {isReferee(roles, shipName) && (
                  <ResetButton appMode={appMode} />
                )}
              </div>
            </>
          )}
        </div>
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
          {/* The star. `decay` near zero means no falloff worth the name, so
              intensity lands as a flat multiplier on every lit surface --
              which is why it has to be about 1. At 6 the sunward face of
              anything pale saturated: Saturn's cloud bands came out as one
              white blob, and Jupiter was washed out enough that ambient had
              already been cut to 0.3 chasing the same fault. Only planets
              are lit at all; ships draw with unlit materials. */}
          <pointLight
            position={[-148e3, 10, 10]}
            intensity={1.0}
            decay={0.01}
            color="#fff7cd"
          />
          {/* Fill for the night side, and the reason a texture's own
              brightness still reads at the terminator. With the star no
              longer six times too strong this can come back up a little. */}
          <ambientLight intensity={0.35} />
          <GrabCamera setCamera={setCamera} />
          <FlyControls
            containerName="main-canvas"
            camera={camera!}
            autoForward={false}
            dragToLook={true}
            movementSpeed={50}
            rollSpeed={CAMERA_ROLL_SPEED}
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

/**
 * Reset the scenario to its loaded state.
 *
 * Reset is reachable from the Scenario Builder and throws away everything added
 * since load, exactly as Exit did. The generic "reset the server?" prompt never
 * said so, so a builder session with unsaved edits gets the same dialog Exit
 * uses; everywhere else keeps the original confirm.
 */
export function ResetButton(args: { appMode: AppMode }) {
  const [confirming, setConfirming] = useState(false);
  const unsavedEdits = useAppSelector(
    (state) =>
      state.tutorial.appMode === AppMode.ScenarioBuilder &&
      state.ui.scenarioDirty,
  );

  return (
    <>
      <button
        className="blue-button"
        onClick={() =>
          unsavedEdits ? setConfirming(true) : resetServer(args.appMode)
        }
      >
        Reset
      </button>
      {confirming && (
        <ConfirmDialog
          title="Unsaved changes"
          message={
            "Resetting returns the scenario to the state it was loaded in. " +
            "Changes that have not been saved will be lost."
          }
          confirmLabel="Discard and reset"
          cancelLabel="Keep editing"
          onConfirm={() => {
            setConfirming(false);
            resetServer(args.appMode, true);
          }}
          onCancel={() => setConfirming(false)}
        />
      )}
    </>
  );
}

export function Exit(args: { email: string | null }) {
  const dispatch = useAppDispatch();
  const [confirming, setConfirming] = useState(false);
  // Only the Scenario Builder has a save to be out of date with, so a running
  // game exits straight away however much its state has moved on.
  const unsavedEdits = useAppSelector(
    (state) =>
      state.tutorial.appMode === AppMode.ScenarioBuilder &&
      state.ui.scenarioDirty,
  );

  const exit = () => {
    setConfirming(false);
    dispatch(setJoinedScenario(null));
    exit_scenario();
    console.log("(Authentication.Logout) Quit scenario");
  };

  const username = args.email ? args.email.split("@")[0] : "";
  return (
    <div className="logout-window">
      <button
        className="blue-button logout-button"
        onClick={() => (unsavedEdits ? setConfirming(true) : exit())}
      >
        Exit {username}
      </button>
      {confirming && (
        <ConfirmDialog
          title="Unsaved changes"
          message={
            "This scenario has changes that have not been saved. " +
            "Leaving now discards them."
          }
          confirmLabel="Discard and exit"
          cancelLabel="Keep editing"
          onConfirm={exit}
          onCancel={() => setConfirming(false)}
        />
      )}
    </div>
  );
}
export default App;
