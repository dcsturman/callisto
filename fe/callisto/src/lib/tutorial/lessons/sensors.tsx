import * as React from "react";
import { RoundButton } from "components/tutorial/RoundButton";
import { SensorAction } from "components/controls/Actions";
import { findShipIn, Lesson, TutorialContext } from "lib/tutorial/lesson";

/** Whether Killer has this sensor order queued against Beowulf. */
const queued = (now: TutorialContext, action: SensorAction) =>
  (now.actions["Killer"]?.sensors ?? []).some((order) => order.action === action && order.target === "Beowulf");

export const sensors: Lesson = {
  id: "sensors",
  title: "Sensors",
  summary: "Finding the enemy, locking on, shaking off a lock, and running dark.",
  scenario: "tutorial-3-sensors.json",
  steps: [
    {
      id: "contacts",
      title: "You can only fight what you can find",
      body: (
        <>
          <p>
            As the GM you can see every ship on the screen, but players crewing a single ship like Killer cannot.
            A ship they have no <em>sensor contact</em> on is only a <em>blip</em>: a faint dot on the screen with
            its name, while its hull, thrust and range read &ldquo;?&rdquo;. In most scenarios every enemy ship
            starts as a blip.
          </p>
          <p>
            A ship can shoot at, lock on to, or jam only the ships it has as contacts. Finding them is the sensor
            operator&apos;s job, but it needs no order from them: at the end of each round, every ship with its
            sensors <em>active</em> tries to find each ship it has not yet found that is closer than{" "}
            <em>Distant</em> range (50,000 km).
          </p>
        </>
      ),
    },
    {
      id: "choose-ship",
      title: "Take Killer",
      target: ["ship-menu"],
      body: <p>The trader Beowulf is 18,000 km away: Long range.</p>,
      task: (
        <>
          Choose <em>Killer</em> from the Ship menu.
        </>
      ),
      done: (now) => now.ui.computerShipName === "Killer",
    },
    {
      id: "eyes-on-us",
      title: "Eyes on us",
      target: ["eyes-on-us"],
      body: (
        <>
          <p>
            The Sensors card starts with who has found <em>us</em>. Bad news: Beowulf got lucky. She has Killer
            as a contact, and more than that, a <b>lock</b>.
          </p>
          <p>
            A lock is a fire-control solution: <b>DM+2 on every attack</b> on the locked ship. Beowulf is only a
            trader, but nobody wants to be a sitting target.
          </p>
        </>
      ),
    },
    {
      id: "emissions",
      title: "How loud you are",
      target: ["emissions"],
      body: (
        <>
          <p>
            Every ship is easier or harder to find depending on what it is giving off. The Emissions gauge adds it
            up as a DM for whoever is looking for you: active sensors +2, each G of thrust +1, the power plant,
            firing, damage, and transmitting (+6, by far the loudest).
          </p>
          <p>
            Against that, a stealthy hull subtracts. Killer is a Harrier with advanced stealth, which is why a
            trader&apos;s civilian sensors should never have found her.
          </p>
        </>
      ),
    },
    {
      id: "find",
      title: "Find Beowulf",
      target: ["next-round"],
      body: (
        <p>
          Killer&apos;s active sensors are on, so she will roll to find Beowulf at the end of the round. With
          military sensors and a trained operator against an unstealthy trader, she can hardly miss.
        </p>
      ),
      task: (
        <>
          Press <RoundButton />, so Killer picks up Beowulf as a contact.
        </>
      ),
      done: (now) => (findShipIn(now.entities, "Killer")?.contacts ?? []).includes("Beowulf"),
    },
    {
      id: "contact",
      title: "What a contact tells you",
      target: ["contact-detail", "ships-panel"],
      body: (
        <>
          <p>
            Beowulf is no longer a blip. In the Ships panel, her hull, thrust and range now read as numbers
            instead of &ldquo;?&rdquo;, and she appears on the Gunner card&apos;s Targets board.
          </p>
          <p>
            The Sensors card&apos;s <b>Contact detail</b> is the operator&apos;s full reading of every ship
            they have found:
          </p>
          <ul>
            <li>
              <b>Her class and size</b>: a Free Trader, 200 tons.
            </li>
            <li>
              <b>thrust</b>: how many G she can pull now, with her design&apos;s rating beside it if damage has
              cut it.
            </li>
            <li>
              <b>sensors</b>: her sensor grade, which says how well she can find and lock on to you.
            </li>
            <li>
              <b>stealth</b> and <b>CM</b> (countermeasures), if she has any: what makes her hard to find and
              hard to keep locked.
            </li>
            <li>
              <b>Damage</b>: each system hit, and how badly, worst first. Nothing shows while she is whole.
            </li>
            <li>
              <b>Her armament</b>: one mark per weapon, its shape the mount and its colour the weapon. Hover over
              one for its name.
            </li>
          </ul>
          <p>
            That is what your gunners and pilot plan with: a fast ship you cannot outrun, or a lightly armed one
            you can close on safely.
          </p>
        </>
      ),
    },
    {
      id: "lock",
      title: "Lock on",
      target: ["sensor-action"],
      body: (
        <p>
          Now Beowulf is a contact, the operator can try for a lock. Each operator does one thing a round, chosen
          from this menu. The order stays in until it works.
        </p>
      ),
      task: (
        <>
          Choose <b>Sensor Lock: Beowulf</b> in the Sensors card&apos;s menu.
        </>
      ),
      done: (now) => queued(now, SensorAction.SensorLock),
    },
    {
      id: "locked",
      title: "Make the lock",
      target: ["next-round"],
      body: (
        <p>
          The lock is rolled at the end of the round: 2D plus the sensors&apos; grade and the operator&apos;s
          skill, against the target&apos;s stealth.
        </p>
      ),
      task: (
        <>
          Press <RoundButton />, and watch for <b>Locks: Beowulf</b> under the menu.
        </>
      ),
      done: (now) => (findShipIn(now.entities, "Killer")?.sensor_locks ?? []).includes("Beowulf"),
    },
    {
      id: "locked-on",
      title: "Locked on",
      target: ["sensor-locks", "target-board"],
      body: (
        <>
          <p>
            Killer has a lock on Beowulf. You can see it in two places: <b>Locks: Beowulf</b> under the
            Sensors card&apos;s menu, and a <b>lock</b> tag on Beowulf&apos;s line in the Gunner card&apos;s Targets
            board, where the gunners will see it.
          </p>
          <p>
            Every attack Killer makes on Beowulf now gets DM+2. The lock holds until Beowulf breaks it, or Killer
            turns her active sensors off.
          </p>
        </>
      ),
    },
    {
      id: "break",
      title: "Shake off her lock",
      target: ["sensor-action"],
      body: (
        <p>
          Beowulf still has her lock on Killer. <b>Break Sensor Lock</b> is offered only against ships that have
          locked on to you. It is an opposed roll, and countermeasures and stealth help the side breaking free.
        </p>
      ),
      task: (
        <>
          Choose <b>Break Sensor Lock: Beowulf</b>, then press <RoundButton />.
        </>
      ),
      done: (now) => !(findShipIn(now.entities, "Beowulf")?.sensor_locks ?? []).includes("Killer"),
    },
    {
      id: "dark",
      title: "Running dark",
      target: ["sensor-emissions-toggles"],
      body: (
        <>
          <p>
            The three switches at the top of the card change how &ldquo;loud&rdquo; Killer is.{" "}
            <b>Active</b> means her <em>active sensors</em> are in use. You need them for two things: to make a
            new sensor contact, and to get a sensor lock. Turning them off is <em>running dark</em>. It makes you
            harder to find, but you cannot make new contacts, and you drop every lock you hold. Contacts you
            already have, you keep.
          </p>
          <p>
            <b>Transmit</b> is the transponder and comms. It is very loud, and makes a ship much easier to find.{" "}
            <b>Hand-off</b> shares your sensor contacts with the other ships on your side. It needs Transmit on,
            and free computer <em>Bandwidth</em> on both your ship and theirs (more on the computer in lesson 8,
            Jumping out).
          </p>
        </>
      ),
      task: (
        <>
          Untick <b>Active</b>, and watch the Emissions gauge drop.
        </>
      ),
      done: (now) => findShipIn(now.entities, "Killer")?.active_sensors === false,
    },
    {
      id: "done",
      title: "Seen, and unseen",
      body: (
        <p>
          That is the sensor operator&apos;s job: find the enemy, lock on, break their locks, and choose when to
          be loud. The other orders in the menu, <b>Jam Comms</b> and <b>Jam Missiles</b>, come up in the
          missiles lesson. Next: the guns.
        </p>
      ),
    },
  ],
};
