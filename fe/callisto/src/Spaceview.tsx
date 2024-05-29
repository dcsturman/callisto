import { useContext } from "react";
import * as THREE from "three";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { KernelSize, Resolution } from "postprocessing";
import { TextureLoader } from "three/src/loaders/TextureLoader";
import { useLoader } from "@react-three/fiber";

import Color from "color";

import { Line, scaleVector } from "./Util";
import { scale, Planet as PlanetType, Entity, EntitiesServerContext } from "./Contexts";

function Planet(args:{planet: Entity}) {
  let planet_details;

  if ("Planet" in args.planet.kind) {
    planet_details = args.planet.kind.Planet as PlanetType;
  } else {
    console.error(`(Spaceview.Planet) Planet ${args.planet.name} not a planet. Details ${JSON.stringify(planet_details)}`);
    return (<></>)
  }

  const color = Color(planet_details.color);
  const intensity_factor = 2.5;

  const radiusMeters = planet_details.radius;
  const radiusUnits = radiusMeters * scale;
  const pos = scaleVector(args.planet.position, scale);

  console.log(`(Spaceview.Planet) Planet ${args.planet.name} details ${JSON.stringify(args.planet)}`);
  console.log(`(Spaceview.Planet) Planet ${args.planet.name} pos ${pos}`);
  console.log(`(Spaceview.Planet) Planet ${args.planet.name} radius ${radiusUnits}`);

  return (
    <>
      <EffectComposer>
        <Bloom
          intensity={1.0} // The bloom intensity.
          blurPass={undefined} // A blur pass.
          kernelSize={KernelSize.LARGE} // blur kernel size
          luminanceThreshold={0.9} // luminance threshold. Raise this value to mask out darker elements in the scene.
          luminanceSmoothing={0.025} // smoothness of the luminance threshold. Range is [0, 1]
          mipmapBlur={true} // Enables or disables mipmap blur.
          resolutionX={Resolution.AUTO_SIZE} // The horizontal resolution.
          resolutionY={Resolution.AUTO_SIZE} // The vertical resolution.
        />
      </EffectComposer>
      <mesh position={pos}>
        <icosahedronGeometry args={[radiusUnits, 15]} />
        <meshBasicMaterial
          color={[
            color.red()/255.0 * intensity_factor,
            color.green()/255.0 * intensity_factor,
            color.blue()/255.0 * intensity_factor,
          ]}
        />
      </mesh>
    </>
  );
}

function Planets(args:{planets: Entity[]}) {
  return (
    <>
      {args.planets.map((planet, index) => (
        <Planet key={planet.name} planet={planet} />
      ))}
    </>
  );
}

function Galaxy() {
  const starColorMap = useLoader(TextureLoader, "galaxy1.png");

  return (
    <>
      <mesh>
        <sphereGeometry args={[5000, 64, 64]} />
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
      <Line start={[-1000, 0, 0]} end={[1000, 0, 0]} color="blue"/>
      <Line start={[0, -1000, 0]} end={[0, 1000, 0]} color="green"/>
      <Line start={[0, 0, -1000]} end={[0, 0, 1000]} />
    </>
  );
}

function SpaceView() {
  const serverEntities = useContext(EntitiesServerContext);
  const planets = serverEntities.entities.planets;

  return (
    <>
      <Axes />
      <Planets planets={planets}/>
      <Galaxy />
    </>
  );
}

export default SpaceView;
