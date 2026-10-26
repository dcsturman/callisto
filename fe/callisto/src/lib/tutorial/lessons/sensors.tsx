import * as React from "react";
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
          Press <b>Next Round</b>, so Killer picks up Beowulf as a contact.
        </>
      ),
      done: (now) => (findShipIn(now.entities, "Killer")?.contacts ?? []).includes("Beowulf"),
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
          Press <b>Next Round</b>, and watch for <b>Locks: Beowulf</b> under the menu.
        </>
      ),
      done: (now) => (findShipIn(now.entities, "Killer")?.sensor_locks ?? []).includes("Beowulf"),
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
          Choose <b>Break Sensor Lock: Beowulf</b>, then press <b>Next Round</b>.
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
            The three switches at the top of the card change how loud Killer is. <b>Active</b> is the radar.
            Turning it off is <b>running dark</b>: quieter, so harder to find, but you find nothing new, and you
            drop every lock you hold. Contacts you already have, you keep.
          </p>
          <p>
            <b>Transmit</b> is the transponder and comms: loud. <b>Hand-off</b> shares your contacts with the
            rest of your side, and needs Transmit on.
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
