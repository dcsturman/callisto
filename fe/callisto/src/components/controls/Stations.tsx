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
export function Stations(args: {ship: Ship}) {
  const roles = useAppSelector((state) => state.user.roles);
  const shipName = useAppSelector((state) => state.user.shipName);
  const operators = args.ship.crew.sensors ?? [];
  const engineers = args.ship.crew.engineers ?? [];

  // The captain inspires other people's checks, so their card carries the
  // round's orders rather than each station keeping its own. Everybody else
  // sees their own queued orders in their own card.
  const captainHoldsTheOrders = hasRole(roles, ViewMode.Captain);

  return (
    <>
      {hasRole(roles, ViewMode.Captain) && (
        <StationCard title="Captain" icon={<GiCaptainHatProfile />} crew={`leadership ${args.ship.crew.leadership ?? 0}`}>
          <CaptainTasks ship={args.ship} />
          {/* Everything queued this round, with the boost ticks beside it:
              the captain is the one person whose job is the whole round. */}
          <QueuedOrders ship={args.ship} />
        </StationCard>
      )}

      {hasRole(roles, ViewMode.Pilot) && (
        <StationCard title="Pilot" icon={<GiShipWheel />} crew={`skill ${args.ship.crew.pilot}`}>
          <PilotStation ship={args.ship} />
          {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Pilot]} />}
        </StationCard>
      )}

      {hasRole(roles, ViewMode.Sensors) &&
        crewSlots(operators.length).map((operator) => (
          <StationCard
            key={`sensors-${operator}`}
            title="Sensors"
            icon={<GiRadarSweep />}
            crew={crewTag(operator, operators.length, operators[operator])}>
            <SensorActionChooser
              ship={args.ship}
              sensorLocks={args.ship.sensor_locks ?? []}
              operator={operator}
            />
            {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Sensors]} />}
            {/* The instruments belong to the station, not to one operator, so
                they hang off the first card. */}
            {operator === 0 && <SensorStation ship={args.ship} />}
          </StationCard>
        ))}

      {hasRole(roles, ViewMode.Gunner) && (
        <>
          <StationCard title="Gunner" icon={<GiLaserTurret />} crew={gunnerTag(args.ship)}>
            <FireControl />
            {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Gunner]} />}
          </StationCard>
          {/* Range, reach and the target's defences: what the gunner had to
              assemble from four other places to choose a shot. */}
          <StationCard title="Targets" icon={<GiCrosshair />}>
            <TargetBoard ship={args.ship} />
            <ShotOdds ship={args.ship} />
          </StationCard>
          {/* The other half of the job: what is coming, and what is left to
              meet it with. */}
          <StationCard title="Incoming" icon={<GiMissileSwarm />}>
            <IncomingBoard ship={args.ship} />
            <Magazine ship={args.ship} />
          </StationCard>
        </>
      )}

      {hasRole(roles, ViewMode.Engineer) && (
        <>
          {crewSlots(engineers.length).map((engineer) => (
            <StationCard
              key={`engineer-${engineer}`}
              title="Engineer"
              icon={<FaCog />}
              crew={engineers.length > 1 ? `#${engineer + 1}` : undefined}>
              <EngineerTasks ship={args.ship} engineer={engineer} />
              {!captainHoldsTheOrders && <QueuedOrders ship={args.ship} only={[ViewMode.Engineer]} />}
            </StationCard>
          ))}
          <StationCard title="Power" icon={<GiLightningTrio />}>
            <PowerBoard ship={args.ship} />
          </StationCard>
          <StationCard title="Damage control" icon={<GiSpanner />}>
            <DamageBoard ship={args.ship} />
          </StationCard>
          {/* The computer is the engineer's to manage, as the power plant
              is: both are budgets, and they read side by side. */}
          <StationCard title="Computer" icon={<GiProcessor />}>
            <ComputerBoard ship={args.ship} />
          </StationCard>
          {/* Only for a ship that has a jump drive to be ready or not. */}
          <StationCard title="Jump" icon={<GiJumpAcross />}>
            <JumpBoard ship={args.ship} />
          </StationCard>
        </>
      )}

      {/* A player sitting on a ship that is not the one they are looking at
          gets told so, rather than quietly giving orders to the wrong crew. */}
      {shipName != null && shipName !== args.ship.name && (
        <p className="stations-note">
          Showing {args.ship.name}. You are aboard {shipName}.
        </p>
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
