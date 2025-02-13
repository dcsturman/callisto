import React from "react";
import Joyride, {
  ACTIONS,
  CallBackProps,
  EVENTS,
  Events,
  ORIGIN,
  STATUS,
  Status,
  Step,
} from "react-joyride";
import { loadScenario } from "./ServerManager";

const steps: Step[] = [
  {
    target: ".mainscreen-container",
    content:
      "Welcome to Callisto! This is the main screen where you can see ships, planets, missiles and other objects in space. You can navigate around using typical flying controls: 'a' and 'd' to turn, 'w' and 's' to move forward and backwards, and 'q' and 'e' to roll.",
    placement: "center",
  },
  {
    target: ".controls-pane",
    content:
      "This is the controls pane. Here you can add ships and then control ships. Most of your time will be spent here.",
    placement: "top-start",
  },
  {
    target: ".button-next-round",
    content: (
      <span>
        This is the button to advance to the next round.
        <p className="tutorial-instruction-text">
          Click it to advance to the next round. Click it a few times just to
          try it out (you should see the Earth and Moon move as it orbits the
          sun.
        </p>
      </span>
    ),
    placement: "top-start",
  },
  {
    target: "#ship-list-dropdown",
    content: (
      <span>
        This dropdown allows us to select a ship to control. We are going to
        select the ship &apos;Killer&apos; as its the only one in the game at this point.{" "}
        <p className="tutorial-instruction-text">
          Select &apos;Killer&apos; from the menu, now.
        </p>
      </span>
    ),
    placement: "right",
  },
  {
    target: "#ship-computer",
    content:
      "We now see the ship's computer for 'Killer'.  You can see here the design (Harrier), hull points, armor, and other stats.  Its current position (x, y, z in km), an option to show ranges from this ship for combat, and its current plan of movement.",
    placement: "auto",
  },
  {
    target: "#computer-window",
    content: "Over in this panel we can use the computer to take some actions.",
    placement: "right",
  },
  {
    target: "#crew-actions-form",
    content:
      "For example, we can set actions by the crew (somewhat limited right now).  Here you can set thrust for the pilot to allocate to dodging, and for assisting gunners.",
    placement: "right",
  },
  {
    target: ".as-form",
    content: "We can manually set an acceleration for 'Killer'.",
    placement: "right",
  },
  {
    target: ".target-entry-form",
    content: (
      <div>
        Or use these controls to have the computer help us plot a course another
        object in space.{" "}
        <p>
          <b>Hint</b>: you can use this computer to get an idea of direction and
          position of other objects, but then manually enter an acceleration to
          customize. For example, suppose you wanted to go full speed to run
          from another ship! Get the direction here, reverse each component of
          the vector, and use that!
        </p>
      </div>
    ),
    placement: "right",
  },
  {
    target: ".mainscreen-container",
    content:
      "Now that we can control a ship, we can start to do something interesting. But first lets create another ship.",
    placement: "center",
  },
  {
    target: "#add-ship-header",
    content: (
      <span>
        Lets add a ship to the scenario.
        <p className="tutorial-instruction-text">
          First click the toggle next to &apos;Add Ship&apos;.
        </p>
      </span>
    ),
    placement: "right",
  },
  {
    target: "#add-ship",
    content: (
      <span className="tutorial-instruction-text">
        Now fill in the form.{" "}
        <p className="tutorial-instruction-text">
          Name it &apos;Beowulf&apos;, leave its position at (0, 0, 0), and give it a
          velocity of (0, 0, 35000) and select the design &apos;Free Trader&apos;.
        </p>
        <p className="tutorial-instruction-text">Click &apos;Add Ship&apos;.</p>
      </span>
    ),
    placement: "right",
  },
  {
    target: ".mainscreen-container",
    content: (
      <span>
        Beowolf is now visible on our screen. You can see the red line showing
        its velocity vector.
        <p className="tutorial-instruction-text">
          Hover over the white glowing dot representing <em>Beowolf</em> to see
          its details. Its tiny so you may need to zoom in a bit.
        </p>
      </span>
    ),
    placement: "center",
  },
  {
    target: "#add-ship-header",
    content: (
      <span>
        Now its time for some combat, but lets clean up the display a bit.
        <p className="tutorial-instruction-text">
          Click the toggle next to &apos;Add Ship&apos; to close those controls.
        </p>
      </span>
    ),
    placement: "right",
  },
  {
    target: ".barbette-button",
    content: (
      <span>
        Notice that <em>Killer&apos;s</em> weapons are disabled right now - because
        we don&apos;t have a valid target! Lets fix that.
      </span>
    ),
    placement: "top",
  },
  {
    target: "#fire-target",
    content: (
      <span>
        This drop-down lets us select any other ship as a target. You&apos;ll see the
        ship name and its range as well.
        <p className="tutorial-instruction-text">
          Select &apos;Beowulf&apos; as the target.
        </p>
      </span>
    ),
    placement: "right",
  },
  {
    target: ".barbette-button",
    content: (
      <span>
        Now that we have a target, we can fire our weapons.
        <p className="tutorial-instruction-text">
          Click the &apos;Barbette&apos; button to fire the particle barbette.
        </p>
      </span>
    ),
    placement: "top",
  },
  {
    target: ".turret-button",
    content: (
      <span>
        We can also fire a missile.
        <p className="tutorial-instruction-text">
          Click the &apos;Missile Turret&apos; button to fire a missile.
        </p>
      </span>
    ),
    placement: "top",
  },
  {
    target: ".button-next-round",
    content: (
      <span>
        Lets see what happened!{" "}
        <p className="tutorial-instruction-text">
          Click the next round button.
        </p>
      </span>
    ),
    placement: "top-start",
  },
  {
    target: "#results-window",
    content: (
      <span>
        You can see the results of the combat here. Note that since{" "}
        <em>Beowulf</em> is at long range, the missile will take time to get
        there.
        <p className="tutorial-instruction-text">
          Click &apos;Okay!&apos; to close the window.
        </p>
      </span>
    ),
    placement: "bottom-start",
  },
  {
    target: ".mainscreen-container",
    content: (
      <span>
        Now you have a quick introduction to Callisto. You can now start to play
        around and explore the game. Good luck!
      </span>
    ),
    placement: "center",
  },
];

const TUTORIAL_SCENARIO = "gs://callisto-scenarios/tutorial.json";

export function Tutorial({
  runTutorial,
  setRunTutorial,
  stepIndex,
  setStepIndex,
  selectAShip,
}: {
  runTutorial: boolean;
  setRunTutorial: (runTutorial: boolean) => void;
  stepIndex: number;
  setStepIndex: (step: number) => void;
  selectAShip: () => void;
  setAuthenticated: (authenticated: boolean) => void;
}) {

  const handleJoyrideCallback = (data: CallBackProps) => {
    const { action, index, origin, status, type } = data;

    console.group("Joyride callback");
    console.log("Joyride callback data:", data);
    console.log("index = ", index);
    console.log("stepIndex = ", stepIndex);
    console.log("runTutorial = " + runTutorial);
    console.groupEnd();

    if (action === ACTIONS.START) {
      loadScenario(TUTORIAL_SCENARIO);
      setStepIndex(0);
    } else if (action === ACTIONS.RESET) {
      setStepIndex(0);
    }
    if (action === ACTIONS.CLOSE && origin === ORIGIN.KEYBOARD) {
      // do something
    }

    if (
      ([EVENTS.STEP_AFTER, EVENTS.TARGET_NOT_FOUND] as Events[]).includes(type)
    ) {
      if (action === ACTIONS.NEXT && stepIndex === 3) {
        selectAShip();
      }
      // Update state to advance the tour
      console.log("(Tutorial) Advance");
      setStepIndex(stepIndex + (action === ACTIONS.PREV ? -1 : 1));
      //, STATUS.SKIPPED
    } else if (([STATUS.FINISHED] as Status[]).includes(status)) {
      // You need to set our running state to false, so we can restart if we click start again.
      console.log("(Tutorial) Reset");
      setStepIndex(0);
      setRunTutorial(false);
    }
  };

  return (
    <div>
      <Joyride
        callback={handleJoyrideCallback}
        run={runTutorial}
        debug={true}
        continuous={true}
        steps={steps}
        spotlightPadding={3}
        spotlightClicks={true}
        disableScrollParentFix={true}
        stepIndex={stepIndex}
        styles={{
          options: {
            backgroundColor: "rgba(20, 20, 90, .8)",
            primaryColor: "#ff3333",
            overlayColor: "rgba(0, 0, 0, 0.2)",
            textColor: "#eeebeb",
            zIndex: 1000,
            beaconSize: 100,
          },
          spotlight: {
            backgroundColor: "rgba(240, 170, 179, .4)",
          },
        }}
      />
    </div>
  );
}

export function RunTutorial({
  restartTutorial,
}: {
  restartTutorial: (fname: string) => void;
}) {
  return (
    <div className="tutorial-button-window">
      <button className="blue-button" onClick={() => restartTutorial(TUTORIAL_SCENARIO)}>
        Restart Tutorial
      </button>
    </div>
  );
}
