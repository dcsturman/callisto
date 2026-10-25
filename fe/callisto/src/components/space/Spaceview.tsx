import * as React from "react";
import { useLoader } from "@react-three/fiber";
import * as THREE from "three";

import { TextureLoader } from "three/src/loaders/TextureLoader";

import { Line, RangeCircle, scaleVector } from "lib/Util";
import { SCALE } from "lib/universal";
import { POST_COLOUR, postsToDraw } from "lib/posts";
import { Planet as PlanetType } from "lib/entities";

import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

import { Planet } from "./Planet";

function Planets(args: { planets: PlanetType[] }) {
  const gravityWell = useAppSelector((state) => state.ui.gravityWells);
  const jumpDistance = useAppSelector((state) => state.ui.jumpDistance);

  return (
    <>
      {args.planets.map((planet) => (
        <Planet
          key={planet.name}
          planet={planet}
          controlGravityWell={gravityWell}
          controlJumpDistance={jumpDistance}
        />
      ))}
    </>
  );
}

function Galaxy() {
  const starColorMap = useLoader(TextureLoader, "/assets/galaxy1.png");

  return (
    <>
      <mesh>
        <sphereGeometry args={[500000, 64, 64]} />
        <meshBasicMaterial
          map={starColorMap}
          side={THREE.BackSide}
          transparent={true}
        />
      </mesh>
    </>
  );
}

function Axes() {
  return (
    <>
      <Line start={[-400000, 0, 0]} end={[400000, 0, 0]} color="blue" />
      <Line start={[0, -400000, 0]} end={[0, 400000, 0]} color="green" />
      <Line start={[0, 0, -400000]} end={[0, 0, 400000]} />
    </>
  );
}

function SpaceView() {
  const entities = useAppSelector(entitiesSelector);
  const planets = entities.planets;

  return (
    <>
      <Axes />
      <Planets planets={planets} />
      {/* Where each Hold side is meant to be: a ring of the radius that
          counts as holding, in purple so it is never read as a range ring
          or a team. */}
      {postsToDraw(entities).map((post) => (
        <RangeCircle
          key={post.key}
          pos={scaleVector(post.position, SCALE)}
          distance={post.radius}
          label={post.label}
          color={POST_COLOUR}
          opacity={0.8}
        />
      ))}
      <Galaxy />
    </>
  );
}

export default SpaceView;
