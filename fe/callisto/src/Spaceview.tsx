import { useContext, useRef, RefObject, createRef } from "react";
import * as THREE from "three";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { KernelSize, Resolution } from "postprocessing";
import { TextureLoader } from "three/src/loaders/TextureLoader";
import { useLoader, useFrame } from "@react-three/fiber";

import Color from "color";

import { Line, scaleVector } from "./Util";
import {
  SCALE,
  Planet as PlanetType,
  EntitiesServerContext,
  EntityToShowContext,
} from "./Universal";

function Planet(args: { planet: PlanetType }) {
  const entityToShow = useContext(EntityToShowContext);
  const radiusMeters = args.planet.radius;
  const radiusUnits = radiusMeters * SCALE;
  const pos = scaleVector(args.planet.position, SCALE);

  type PlanetTemplateType = Record<
    string,
    { texture: THREE.Texture; rotation: number }
  >;

  const PLANET_TEMPLATES: PlanetTemplateType = {
    "!earth": {
      texture: useLoader(TextureLoader, "/assets/earthmap1k.jpg"),
      rotation: 0.002,
    },
    "!sun": {
      texture: useLoader(TextureLoader, "/assets/sunmap.jpg"),
      rotation: 0.0,
    },
    "!moon": {
      texture: useLoader(TextureLoader, "/assets/moonmap1k.jpg"),
      rotation: 0.0,
    },
  };

  const bumpMap = useLoader(TextureLoader,"/assets/earthbump1k.jpg");
  const specMap = useLoader(TextureLoader,"/assets/earthspec1k.jpg");


  let texture_details = PLANET_TEMPLATES[args.planet.color];

  const ref = useRef<THREE.Mesh>(null);
  useFrame(() => {
    if (texture_details != null && ref.current != null) {
      ref.current.rotation.y += texture_details.rotation;
    }
  });

  if (texture_details != null) {
    return (
      <mesh
        ref={ref}
        rotation-y={1}
        position={pos}
        onPointerOver={() => entityToShow.setEntityToShow(args.planet)}
        onPointerLeave={() => entityToShow.setEntityToShow(null)}>
        <icosahedronGeometry args={[radiusUnits, 15]} />
        <meshStandardMaterial
          map={texture_details.texture}
          //bumpMap={bumpMap}
          //bumpScale={0.005}
          //specularMap={specMap}
          //specular={"grey"}
          side={THREE.BackSide}
          //transparent={true}
        />
      </mesh>
    );
  } else {
    const color = Color(args.planet.color);
    const intensity_factor = 2.5;

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
        <mesh
          position={pos}
          onPointerOver={() => entityToShow.setEntityToShow(args.planet)}
          onPointerLeave={() => entityToShow.setEntityToShow(null)}>
          <icosahedronGeometry args={[radiusUnits, 15]} />
          <meshBasicMaterial
            color={[
              (color.red() / 255.0) * intensity_factor,
              (color.green() / 255.0) * intensity_factor,
              (color.blue() / 255.0) * intensity_factor,
            ]}
          />
        </mesh>
      </>
    );
  }
}

function Planets(args: { planets: PlanetType[] }) {
  return (
    <>
      {args.planets.map((planet, index) => (
        <Planet key={planet.name} planet={planet} />
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
      <Line start={[-1000, 0, 0]} end={[1000, 0, 0]} color="blue" />
      <Line start={[0, -1000, 0]} end={[0, 1000, 0]} color="green" />
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
      <Planets planets={planets} />
      <Galaxy />
    </>
  );
}

export default SpaceView;
