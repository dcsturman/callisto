import React from "react";
import { useContext } from "react";
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
import { ViewMode, ViewContext } from "lib/universal";

const steps: Step[] = [
  {
    target: ".mainscreen-container",
    content:
      <div > Welcome to Callisto! This is the main screen where you can see ships, planets, missiles and other objects in space. You can navigate around using typical flying controls:
        <br />
        <dl>
          <dt>&apos;a&apos;, &apos;d&apos;</dt> 
          <dd>to turn left or right</dd>
          <dt>&apos;w&apos;, &apos;s&apos;</dt> 
          <dd>to move forward or back</dd> 
          <dt>&apos;q&apos;, &apos;e&apos;</dt> 
          <dd>to roll</dd> 
          <dt>&apos;r&apos;, &apos;f&apos;</dt>
          <dd>to raise or lower</dd>
        </dl>
        <br />
        You can also rotate your view using the mouse by holding down the mouse button.  Try closing this window and moving your view around space.  You can return to the next step in the tutorial by clicking on the glowing red
        tutorial button.  
        <br />
        <br />
        In this tutorial, information is shown in white text as you see here.  Instructions you are to follow are shown &nbsp;
        <text className="tutorial-instruction-text">in red text like this.</text>
      </div>,
    placement: "center",
  },
  {
    target: ".controls-pane",
    content:
      "This is the controls pane. Here you can add ships and then control ships. Most of your time will be spent here.",
    placement: "top-start",
  },
  {
    target: ".admin-button-window",
    content: 
      <div>These are the <em>user controls</em> including: 
        <ul><li>the scenario name (currently &apos;Tutorial&apos;). For other scenarios this name will be unique and should be used to tell others where to join you from the scenarios page.</li>
        <li>the users in the scenario and which role/ship they are playing.  This is not shown when only a single user is in a scenario.</li>
        <li> the role (e.g. crew position) you are playing (in this case &apos;General&apos;)</li>
        <li>and the specific ship you are controlling.</li>
        <li>a button to reset the scenario (only shown in the &apos;General&apos; role)</li>
        <li>a button to exit the scenario</li>
        </ul>
        The controls you are shown will vary based on your role and ship. 
 
      </div>,
    placement: "top",
  },
  {
    target: ".view-controls-window",
    content: <span>These are the <em>view controls</em>.  You can toggle the display of gravity wells and the 100 diameter limit for all gravitational bodies (planets, moons, etc). A ship cannot jump while within this limit.  Currently gravity does not affect ships.</span>,
    placement: "bottom-start",
  },
  {
    target: ".button-next-round",
    content: (
      <span>
        This is the button to advance to the next round.
        <p className="tutorial-instruction-text">
          Click it to advance to the next round. Click it a few times just to
          try it out (you should see the Earth and Moon move as they orbit the
          sun and the moon orbits the Earth).
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
      <span>We now see the ship&apos; computer for &apos;Killer&apos;.  You can see here the design (Harrier), hull points, armor, and other stats.  
        Its current position (x, y, z in km), velocity, an option to show ranges from this ship for combat, and its current plan of of acceleration.
        <br/>
        <b>Note!</b> Position in Callisto is always in <em>kilometers</em>, velocity is in <em>m/s</em>, and acceleration is in <em>G&apos;s</em>.
        </span>,
    placement: "auto",
  },
  {
    target: "#computer-window",
    content: <span>Over in this panel we can use the computer to take some actions. This panel is central to successfully navigating a ship through space so we&apos;ll go into more detail now.</span>,
    placement: "right",
  },
  {
    target: "#crew-actions-window",
    content:
      <span>
        We can set actions by the crew.  Here you can set maneuver points for the pilot to allocate to either dodging or for assisting gunners.
        You can select an operation for the sensor operator, or have the engineer initiate a jump (if outside the 100 diameter limit of all planets).  A successful jump removes the ship from the scenario.
      </span>,
    placement: "right",
  },
  {
    target: ".as-form",
    content: <span>
      We can manually set an acceleration for &apos;Killer&apos; with an acceleration vector (x, y, z) in the three input boxes provided. The total magnitude of the acceleration vector must be not more than the total <em>Maneuver</em> capability 
      of the ship, less any maneuver points allocated to dodging or assisting gunners. 
      <br />
      <br />
      Setting this vector manually, however, tends to be an action used only when all other tools aren&apos;t 
      quite working right as you&apos;ll find its very difficult for a human to figure out the right acceleration.  This control is useful 
      when, for example, you have an acceleration plan <em>towards</em> a target and want to go in the opposite direction (e.g. run away from an attacker, or flee to outside the 100D limit of a planet).
      </span>,
    placement: "right",
  },
  {
    target: ".target-entry-form",
    content: (
      <div>
        The Nav Target function is the most powerful assist in navigating in Callisto.  To use this control, select a target and have the computer plot a course.
        Choosing a target (ship or planet) automatically populates the position, velocity, and acceleration fields.  If you don&apos;t want to end up directly on top of the target,
        use the <em>Standoff</em> field to end the course some set distance away.  
      </div>
    ),
    placement: "right-end",
  },
  {
    target: ".target-entry-form",
    content: (
      <div>
        The computer will take into account the ship&apos;s <em>Maneuver</em> maximum and deduct any maneuver points allocated to <em>Pilot Actions</em>.
        The computer will also assume you want to intercept your target so will aim to finish with the same velocity as the target.
      </div>
    ),
    placement: "right-end",
  },

  {
    target: ".target-entry-form",
    content: 
      <span>
        <b>Hint</b>: you can use this computer to get an idea of direction and
        position of other objects, but then manually enter an acceleration to
        customize. For example, suppose you wanted to go full speed to run
        from another ship! Get the direction here, reverse each component of
        the vector, and use that!
      </span>,
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
      <span >
        Now fill in the form.{" "}
        <p className="tutorial-instruction-text">
          Name your ship &apos;Beowulf&apos;, leave its position at (0, 0, 0), and give it a
          velocity of (0, 0, 35000) and select the design &apos;Free Trader&apos;.
        </p>
        <p><b>Note!</b> Once you select a design if you hover over it, the design details will pop up in green text.</p>
        <p className="tutorial-instruction-text">Click &apos;Add Ship&apos;.</p>
      </span>
    ),
    placement: "right",
  },
  {
    target: "#add-ship",
    content: (
      <span>
        Oops, lets put this ship a bit farther away!
        <br/>
        <br/>To do this, just type Beowulf back into the name input.  You will see the name turn 
        green and load the previous design we entered for the ship.  We can now change the position.
        <p className="tutorial-instruction-text">
          Change:
          <ul>
            <li>position: (0, 30000, -30000)</li>
            <li>click &apos;Update&apos;.</li>
          </ul>
        </p>
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
    target: "#fire-target",
    content: (
      <span>
       We see that <em>Beowulf</em> is at <em>Distant</em> range.  In Callisto, ranges are short, medium, long, very long, and distant.
       Distant range will out of the range of our <em>Particle Barbette</em>&nbsp;
       so lets get closer before we engage!
      </span>
    ),
    placement: "top",
  },
  {
    target: "#show-range-checkbox",
    content: (
      <span>
        You can see ranges graphically by clicking this checkbox.  
        <br/>
        <p className="tutorial-instruction-text">
          Toggle the checkbox on to show ranges, then toggle it off again.
        </p>
        In this view <em>short</em> range is <em>very</em> close,
        so you will have to zoom in or look very closely to see it! Short range is 1,250 km but space is big!
      </span>
    ),
    placement: "top",
  },
  {
    target: "#fire-target",
    content: (
      <span>
       Distant range will out of the range of our <em>Particle Barbette</em>&nbsp;
       so lets get closer before we engage!
      </span>
    ),
    placement: "top",
  },
  {
    target: ".target-entry-form",
    content: 
      <span>
        Lets get closer by having the navigation computer plot a course to <em>Beowulf</em>.
        <p className="tutorial-instruction-text">
          Select &apos;Beowulf&apos; as the destination and then click &apos;Compute to see the proposed plan.
        </p>
      </span>,
    placement: "right",
  },
  {
    target: "#proposed-plan-region",
    content: 
      <span>
        Notice a &apos;plan&apos; is two accelerations.  For some amount of time (in seconds) we will accelerate on a 3D vector.  The magnitude (i.e. total force) of that acceleration vector
        never exceed the ships current maximum acceleration. The acceleration shows in m/s<sup>2</sup> the acceleration in each direction.  Ship&apos;s maneuver rating however is in G&apos;s:  
        <br />1G = 9.8 m/s<sup>2</sup>.  
        <br />
        <br />
        If a ship takes damage to its powerplant or maneuver drive, this maximum acceleration may be reduced!  The resulting plan is 
        sketched out in space as an orange line.  In this case you&apos;ll see it is curving as we&apos;re matching the current known velocity of <em>Beowulf</em>.  
        <p className = "tutorial-instruction-text">
        Now that we have a plan we need to assign the plan to <em>Killer</em> by selecting the &apos;Assign Plan&apos; button.
        </p>
      </span>,
    placement: "right",
  },
  {
    target: "#current-plan-heading", 
    content:
      <span>
        You can see the assigned plan here under the &apos;Current Plan&apos; heading.
      </span>,
    placement: "right",
  },
  {
    target: ".button-next-round",
    content: (
      <span>
        Notice that the display is showing our course and taking the <em>Beowulf&apos;s</em> velocity into account!
        <p className="tutorial-instruction-text">Advance the scenario 5 rounds by clicking on the <em>Next Round</em> button.</p>
      </span>
    ),
    placement: "top-start",
  },
  {
    target: "#particle-barbette-button",
    content: (
      <span>
        Now we are short range! Lets fire our weapons! <br />
        (notice also we could have the <em>Particle Barbette&apos;s</em> gunner do a called shot, but we won&apos;t try that now.)
        <p className="tutorial-instruction-text">
          Click the &apos;Barbette&apos; button to fire the particle barbette.
        </p>
      </span>
    ),
    placement: "top",
  },
  {
    target: "#missile-single-turret-button",
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
        You can see the results of the combat here. Because{" "}
        <em>Beowulf</em> is at short range, the missile impacts right away!  At longer ranges
        you would see the missile track towards the target.
        <p className="tutorial-instruction-text">
          Click &apos;Okay!&apos; to close the window.
        </p>
      </span>
    ),
    placement: "top-start",
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

  const viewContext = useContext(ViewContext);

  const handleJoyrideCallback = (data: CallBackProps) => {
    const { action, index, origin, status, type } = data;

    console.group("Joyride callback");
    console.log("Joyride callback data:", data);
    console.log("index = ", index);
    console.log("stepIndex = ", stepIndex);
    console.log("runTutorial = " + runTutorial);
    console.groupEnd();

    if (action === ACTIONS.START) {
      setStepIndex(0);
      viewContext.setRole(ViewMode.General);
      viewContext.setShipName(null);
    } else if (action === ACTIONS.RESET) {
      setStepIndex(0);
    }
    if (action === ACTIONS.CLOSE && origin === ORIGIN.KEYBOARD) {
      // do something
    }

    if (
      ([EVENTS.STEP_AFTER, EVENTS.TARGET_NOT_FOUND] as Events[]).includes(type)
    ) {
      if (action === ACTIONS.NEXT && stepIndex === 5) {
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
          tooltipContent: {
            textAlign: "left",
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
