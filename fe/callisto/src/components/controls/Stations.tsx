import * as React from "react";

import {Ship} from "lib/entities";
import {ViewMode, hasRole} from "lib/view";
import {useAppSelector} from "state/hooks";

import {StationCard} from "components/controls/StationCard";
import {
  GiCaptainHatProfile,
  GiCrosshair,
  GiMissileSwarm,
  GiJumpAcross,
  GiLaserTurret,
  GiLightningTrio,
  GiProcessor,
  GiRadarSweep,
  GiShipWheel,
  GiSpanner,
} from "react-icons/gi";
import {FaCog} from "react-icons/fa";
import {CaptainTasks} from "components/controls/CaptainTasks";
import {PilotStation, SensorActionChooser} from "components/controls/ShipComputer";
import {SensorStation} from "components/controls/SensorStation";
import {EngineerTasks} from "components/controls/EngineerTasks";
import {PowerBoard} from "components/controls/PowerBoard";
import {DamageBoard} from "components/controls/DamageBoard";
import {IncomingBoard} from "components/controls/IncomingBoard";
import {Magazine} from "components/controls/Magazine";
import {ShotOdds} from "components/controls/ShotOdds";
import {TargetBoard} from "components/controls/TargetBoard";
import {ComputerBoard} from "components/controls/ComputerBoard";
import {JumpBoard} from "components/controls/JumpBoard";
import {useAppDispatch} from "state/hooks";
import {moveCard, resetCardOrder} from "state/uiSlice";

/** A stable empty list, so the selector does not re-render every tick. */
const EMPTY_ORDER: string[] = [];
import {FireControl, QueuedOrders} from "components/controls/WeaponUse";

/**
 * Every station this player is working, each in the same card.
 *
 * One place and one shape for all of them. The layout used to depend on the
 * role: a gunner's fire controls were an accordion in the left column, a
 * pilot's helm was a bare heading in a pop-up, and the referee -- who works
 * every station -- got a second copy of the lot in a window of its own while
 * the left column sat empty. Now the referee simply has more cards.
 *
 * The cards are siblings of the ship's dossier in one wrapping column: they
 * fill the space under it first and start a second column only when that one
 * is full, so the console is never wider than it needs to be.
 */
/** Title and glyph per card, so the ordered list can still render itself. */
const CARD_LOOK: Record<string, {title: string; icon?: React.ReactNode}> = {
  captain: {title: "Captain", icon: <GiCaptainHatProfile />},
  pilot: {title: "Pilot", icon: <GiShipWheel />},
  sensors: {title: "Sensors", icon: <GiRadarSweep />},
  gunner: {title: "Gunner", icon: <GiLaserTurret />},
  targets: {title: "Targets", icon: <GiCrosshair />},
  incoming: {title: "Incoming", icon: <GiMissileSwarm />},
  engineer: {title: "Engineer", icon: <FaCog />},
  power: {title: "Power", icon: <GiLightningTrio />},
  damage: {title: "Damage control", icon: <GiSpanner />},
  computer: {title: "Computer", icon: <GiProcessor />},
  jump: {title: "Jump", icon: <GiJumpAcross />},
};

/**
 * The player's arrangement, with anything it has not seen on the end.
 *
 * A card added in a later version must appear rather than vanish for
 * everyone who has arranged their console, so an unknown id sorts last in
 * its natural order rather than being dropped.
 */
export function orderCards<T extends {id: string}>(cards: T[], order: string[]): T[] {
  if (order.length === 0) {
    return cards;
  }
  const rank = new Map(order.map((id, index) => [id, index]));
  return [...cards].sort((a, b) => {
    const ra = rank.get(a.id) ?? Number.MAX_SAFE_INTEGER;
    const rb = rank.get(b.id) ?? Number.MAX_SAFE_INTEGER;
    if (ra !== rb) {
      return ra - rb;
    }
    return cards.indexOf(a) - cards.indexOf(b);
  });
}

/** Who is working this station, where the ship carries more than one. */
function cardCrew(id: string, ship: Ship, operators: number[], engineers: unknown[]): string | undefined {
  if (id === "captain") {
    return `leadership ${ship.crew.leadership ?? 0}`;
  }
  if (id === "pilot") {
    return `skill ${ship.crew.pilot}`;
  }
  if (id === "gunner") {
    return gunnerTag(ship);
  }
  if (id.startsWith("sensors-")) {
    const operator = Number(id.slice("sensors-".length));
    return crewTag(operator, operators.length, operators[operator]);
  }
  if (id.startsWith("engineer-")) {
    const engineer = Number(id.slice("engineer-".length));
    return engineers.length > 1 ? `#${engineer + 1}` : undefined;
  }
  return undefined;
}

export function Stations(args: {ship: Ship}) {
  const roles = useAppSelector((state) => state.user.roles);
  const cardOrder = useAppSelector((state) => state.ui.cardOrder ?? EMPTY_ORDER);
  const dispatch = useAppDispatch();
  const operators = args.ship.crew.sensors ?? [];
  const engineers = args.ship.crew.engineers ?? [];

  // The captain inspires other people's checks, so their card carries the
  // round's orders rather than each station keeping its own. Everybody else
  // sees their own queued orders in their own card.
  const captainHoldsTheOrders = hasRole(roles, ViewMode.Captain);

  // Every card this player works, each with a stable id so an arrangement
  // survives a reload and a card added later can be slotted in.
  const cards: {id: string; node: React.ReactNode}[] = [];

  if (hasRole(roles, ViewMode.Captain)) {
    cards.push({
      id: "captain",
      node: (
        <>
          <CaptainTasks ship={args.ship} />
          {/* Everything queued this round, with the boost ticks beside it:
              the captain is the one person whose job is the whole round. */}
          <QueuedOrders ship={args.ship} />
        </>
      ),
    });
  }

  if (hasRole(roles, ViewMode.Pilot)) {
    cards.push({
      id: "pilot",
      node: (
        <>
          <PilotStation ship={args.ship} />
          {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Pilot]} />}
        </>
      ),
    });
  }

  if (hasRole(roles, ViewMode.Sensors)) {
    for (const operator of crewSlots(operators.length)) {
      cards.push({
        id: `sensors-${operator}`,
        node: (
          <>
            <SensorActionChooser
              ship={args.ship}
              sensorLocks={args.ship.sensor_locks ?? []}
              operator={operator}
            />
            {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Sensors]} />}
            {/* The instruments belong to the station, not to one operator, so
                they hang off the first card. */}
            {operator === 0 && <SensorStation ship={args.ship} />}
          </>
        ),
      });
    }
  }

  if (hasRole(roles, ViewMode.Gunner)) {
    cards.push({
      id: "gunner",
      node: (
        <>
          <FireControl />
          {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Gunner]} />}
        </>
      ),
    });
    // Range, reach and the target's defences: what the gunner had to
    // assemble from four other places to choose a shot.
    cards.push({
      id: "targets",
      node: (
        <>
          <TargetBoard ship={args.ship} />
          <ShotOdds ship={args.ship} />
        </>
      ),
    });
    // The other half of the job: what is coming, and what is left to meet it.
    cards.push({
      id: "incoming",
      node: (
        <>
          <IncomingBoard ship={args.ship} />
          <Magazine ship={args.ship} />
        </>
      ),
    });
  }

  if (hasRole(roles, ViewMode.Engineer)) {
    for (const engineer of crewSlots(engineers.length)) {
      cards.push({
        id: `engineer-${engineer}`,
        node: (
          <>
            <EngineerTasks ship={args.ship} engineer={engineer} />
            {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Engineer]} />}
          </>
        ),
      });
    }
    cards.push({id: "power", node: <PowerBoard ship={args.ship} />});
    cards.push({id: "damage", node: <DamageBoard ship={args.ship} />});
    // The computer is the engineer's to manage, as the power plant is: both
    // are budgets, and they read side by side.
    cards.push({id: "computer", node: <ComputerBoard ship={args.ship} />});
    // Only for a ship that has a jump drive to be ready or not.
    cards.push({id: "jump", node: <JumpBoard ship={args.ship} />});
  }

  const ordered = orderCards(cards, cardOrder);
  const ids = ordered.map((card) => card.id);

  return (
    <>
      {ordered.map((card, index) => {
        const look = CARD_LOOK[card.id.replace(/-\d+$/, "")] ?? {title: card.id};
        return (
          <StationCard
            key={card.id}
            title={look.title}
            icon={look.icon}
            crew={cardCrew(card.id, args.ship, operators, engineers)}
            onMoveUp={
              index === 0
                ? undefined
                : () => dispatch(moveCard({id: card.id, delta: -1, current: ids}))
            }
            onMoveDown={
              index === ordered.length - 1
                ? undefined
                : () => dispatch(moveCard({id: card.id, delta: 1, current: ids}))
            }>
            {card.node}
          </StationCard>
        );
      })}
      {cardOrder.length > 0 && (
        <button
          type="button"
          className="card-order-reset"
          onClick={() => dispatch(resetCardOrder())}>
          Reset card order
        </button>
      )}
    </>
  );
}

/** A station always has at least one seat, even with nobody rated in it. */
const crewSlots = (count: number): number[] =>
  Array.from({length: Math.max(1, count)}, (_, index) => index);

const crewTag = (index: number, count: number, skill: number | undefined): string =>
  count > 1 ? `#${index + 1} · skill ${skill ?? 0}` : `skill ${skill ?? 0}`;

/** Gunners are one per mount, so the card names them as a row of numbers. */
const gunnerTag = (ship: Ship): string | undefined => {
  const gunners = ship.crew.gunnery ?? [];
  return gunners.length === 0 ? undefined : `skill ${gunners.join("/")}`;
};

export default Stations;
