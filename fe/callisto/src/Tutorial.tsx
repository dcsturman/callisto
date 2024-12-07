
import React, { useState } from 'react';
import Joyride, { ACTIONS, CallBackProps, EVENTS, Events, ORIGIN, STATUS, Status } from 'react-joyride';

const steps: any[] = [
    {
        target: '.mainscreen-container',
        content: 'Welcome to Callisto! This is the main screen where you can see ships, planets, missiles and other objects in space. You can navigate around using typical flying controls: \'a\' and \'d\' to turn, \'w\' and \'s\' to move forward and backwards, and \'q\' and \'e\' to roll.',
        placement: 'center',
    },
    {
        target: '.controls-pane',
        content: 'This is the controls pane. Here you can add ships and then control ships. Most of your time will be spent here.',
        placement: 'top-start',
    },
    {
        target: '.button-next-round',
        content: <span>This is the button to advance to the next round.<p className="tutorial-instruction-text">Click it to advance to the next round.  Click it a few times just to try it out (you should see the Earth and Moon move as it orbits the sun.</p></span>,
        placement: 'top-start',
    },
    {
        target: '#ship-list-dropdown',
        content: <span>This dropdown allows us to select a ship to control. We are going to select the ship 'Killer' as its the only one in the game at this point. <p  className="tutorial-instruction-text">Select 'Killer' from the menu, now.</p></span>,
        placement: 'right',
    },
    {
        target: '#ship-computer',
        content: "We now see the ship's computer for 'Killer'.  You can see here the design (Harrier), hull points, armor, and other stats.  Its current position (x, y, z in km), an option to show ranges from this ship for combat, and its current plan of movement.",
        placement: 'auto',
    },
    {
        target: '#computer-window',
        content: "Over in this panel we can use the computer to take some actions.",
        placement: 'right',
    },
    {
        target: "#crew-actions-form",
        content: "For example, we can set actions by the crew (somewhat limited right now).  Here you can set thrust for the pilot to allocate to dodging, and for assisting gunners.",
        placement: 'right',
    },
    {
        target: ".as-form",
        content: "We can manually set an acceleration for 'Killer'.",
        placement: 'right'
    },
    {
        target: ".target-entry-form",
        content: <div>Or use these controls to have the computer help us plot a course another object in space. <p><b>Hint</b>: you can use this computer to get an idea of direction and position of other objects, but then manually enter an acceleration to customize. For example, suppose you wanted to go full speed to run from another ship! Get the direction here, reverse each component of the vector, and use that!</p></div>,
        placement: 'right',
    },
    {
        target: ".mainscreen-container",
        content: "Now that we can control a ship, we can start to do something interesting. But first lets create another ship.",
        placement: 'center',
    },
    {
        target: "#add-ship-header",
        content: <span>Lets add a ship to the scenario.<p className="tutorial-instruction-text">First click the toggle next to 'Add Ship'.</p></span>,
        placement: 'right'
    },
    {
        target: "#add-ship",
        content: <span className="tutorial-instruction-text">Now fill in the form.  <p className="tutorial-instruction-text">Name it 'Beowulf', leave its position at (0, 0, 0), and give it a velocity of (0, 0, 35000) and select the design 'Free Trader'.</p><p className="tutorial-instruction-text">Click 'Add Ship'.</p></span>,
        placement: 'right'
    },
    {
        target: ".mainscreen-container",
        content: <span>Beowolf is now visible on our screen.  You can see the red line showing its velocity vector.<p className="tutorial-instruction-text">Hover over the white glowing dot representing <em>Beowolf</em> to see its details.  Its tiny so you may need to zoom in a bit.</p></span>,
        placement: 'center',
    },
    {
        target: "#add-ship-header",
        content: <span>Now its time for some combat, but lets clean up the display a bit.<p className="tutorial-instruction-text">Click the toggle next to 'Add Ship' to close those controls."</p></span>,
        placement: 'right',
    },
    {
        target: ".barbette-button",
        content: <span>Notice that <em>Killer's</em> weapons are disabled right now - because we don't have a valid target!  Lets fix that.</span>,
        placement: 'top',
    },
    {
        target: "#fire-target",
        content: <span>This drop-down lets us select any other ship as a target. You'll see the ship name and its range as well.<p className="tutorial-instruction-text">Select 'Beowulf' as the target.</p></span>,
        placement: 'right',
    },
    {
        target: ".barbette-button",
        content: <span>Now that we have a target, we can fire our weapons.<p className="tutorial-instruction-text">Click the 'Barbette' button to fire the particle barbette.</p></span>,
        placement: 'top',
    },
    {
        target: ".turret-button",
        content: <span>We can also fire a missile.<p className="tutorial-instruction-text">Click the 'Missile Turret' button to fire a missile.</p></span>,
        placement: 'top',
    },
    {
        target: '.button-next-round',
        content: <span>Lets see what happened! <p className="tutorial-instruction-text">Click the next round button.</p></span>,
        placement: 'top-start',
    },
    {
        target: "#results-window",
        content: <span>You can see the results of the combat here.  Note that since <em>Beowulf</em> is at long range, the missile will take time to get there.<p className="tutorial-instruction-text">Click 'Okay!' to close the window.</p></span>,
        placement: 'bottom-start',
    },
    {
        target: ".mainscreen-container",
        content: <span>Now you have a quick introduction to Callisto.  You can now start to play around and explore the game.  Good luck!</span>,
        placement: 'center',
    },


]

export function Tutorial({runTutorial, setRunTutorial, selectAShip}: {runTutorial: boolean, setRunTutorial: (runTutorial: boolean) => void, selectAShip: () => void}) {
    const [stepIndex, setStepIndex] = useState(0);

    const handleJoyrideCallback = (data: CallBackProps) => {
        const { action, index, origin, status, type } = data;

        if (action === ACTIONS.CLOSE && origin === ORIGIN.KEYBOARD) {
          // do something
        }
    
        if (([EVENTS.STEP_AFTER, EVENTS.TARGET_NOT_FOUND] as Events[]).includes(type)) {
            if (action === ACTIONS.NEXT && index === 23) {
                selectAShip();
            }
          // Update state to advance the tour

          //setStepIndex(index + (action === ACTIONS.PREV ? -1 : 1));
          //, STATUS.SKIPPED
        } else if (([STATUS.FINISHED] as Status[]).includes(status)) {
          // You need to set our running state to false, so we can restart if we click start again.
          setRunTutorial(false);
        }
    
        console.groupCollapsed(type);
        console.log(data); //eslint-disable-line no-console
        console.groupEnd();
      };


    return (
        <div>
            <Joyride callback={handleJoyrideCallback} run={runTutorial} debug={true} continuous={true} steps={steps}
            spotlightPadding={3}
            spotlightClicks={true}
            disableScrollParentFix={true}
            styles = {{
                options: {
                    backgroundColor: "rgba(20, 20, 90, .8)",
                    primaryColor:"#505050",
                    overlayColor: "rgba(0, 0, 0, 0.2)",
                    textColor: "#eeebeb",
                    zIndex: 1000
                },
                spotlight: {
                    backgroundColor: "rgba(170, 170, 179, .4)"
                }
            }}
            />
        </div>
    )
}

export function RunTutorial({restartTutorial}: {restartTutorial: () => void}) {
    return (
            <div className="tutorial-button-window">
                <button className="blue-button" onClick={restartTutorial}>Restart Tutorial</button>
            </div>
    )
}