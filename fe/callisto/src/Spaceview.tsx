import { useContext, useRef } from "react";
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

function Planet(args: { planet: PlanetType; controlGravityWell: boolean }) {
  const controlGravityWell = args.controlGravityWell;
  const entityToShow = useContext(EntityToShowContext);
  const radiusMeters = args.planet.radius;
  const radiusUnits = radiusMeters * SCALE;
  const pos = scaleVector(args.planet.position, SCALE);

  const GRAVITY_WELL_OPACITY = 0.15;

  // Render the gravity wells at the given distance from the planet.
  // The order is the render order which is ugly but is critical to avoid some of the transparent 
  // gravity wells from not being rendered.
  function gravityWell(distance: number, order: number) {
    return (
      <>
        {controlGravityWell && (
          <mesh position={pos} renderOrder={order} >
            <sphereGeometry args={[distance * SCALE, 15, 15]} />
            <meshLambertMaterial color="#ffffff" opacity={GRAVITY_WELL_OPACITY} alphaToCoverage={true} shadowSide={THREE.FrontSide} transparent={true} side={THREE.FrontSide} />
          </mesh>
        )}
      </>
    );
  }
  type PlanetTemplateType = Record<
    string,
    {
      texture: THREE.Texture;
      rotation: number;
      bumpMap: THREE.Texture | undefined;
      bumpScale: number;
      specularMap: THREE.Texture | undefined;
      specular: THREE.Color | undefined;
    }
  >;

  const PLANET_TEMPLATES: PlanetTemplateType = {
    "!earth": {
      texture: useLoader(TextureLoader, "/assets/earthmap1k.jpg"),
      rotation: 0.002,
      bumpMap: useLoader(TextureLoader, "/assets/earthbump1k.jpg"),
      bumpScale: 0.05,
      specularMap: useLoader(TextureLoader, "/assets/earthspec1k.jpg"),
      specular: new THREE.Color("grey"),
    },
    "!sun": {
      texture: useLoader(TextureLoader, "/assets/sunmap.jpg"),
      rotation: 0.0,
      bumpMap: useLoader(TextureLoader, "/assets/sunmap.jpg"),
      bumpScale: 0.05,
      specularMap: undefined,
      specular: undefined,
    },
    "!moon": {
      texture: useLoader(TextureLoader, "/assets/moonmap1k.jpg"),
      rotation: 0.0,
      bumpMap: useLoader(TextureLoader, "/assets/moonbump1k.jpg"),
      bumpScale: 0.002,
      specularMap: undefined,
      specular: undefined,
    },
    "!mercury": {
      texture: useLoader(TextureLoader, "/assets/mercurymap.jpg"),
      rotation: 0.0,
      bumpMap: useLoader(TextureLoader, "/assets/mercurybump.jpg"),
      bumpScale: 0.005,
      specularMap: undefined,
      specular: undefined,
    },
    "!venus": {
      texture: useLoader(TextureLoader, "/assets/venusmap.jpg"),
      rotation: 0.0005,
      bumpMap: useLoader(TextureLoader, "/assets/venusbump.jpg"),
      bumpScale: 0.005,
      specularMap: undefined,
      specular: undefined,
    },
    "!mars": {
      texture: useLoader(TextureLoader, "/assets/marsmap1k.jpg"),
      rotation: 0.002,
      bumpMap: useLoader(TextureLoader, "/assets/marsbump1k.jpg"),
      bumpScale: 0.05,
      specularMap: undefined,
      specular: undefined,
    },
    "!jupiter": {
      texture: useLoader(TextureLoader, "/assets/jupitermap.jpg"),
      rotation: 0.005,
      bumpMap: useLoader(TextureLoader, "/assets/jupitermap.jpg"),
      bumpScale: 0.02,
      specularMap: undefined,
      specular: undefined,
    },
    "!saturn": {
      texture: useLoader(TextureLoader, "/assets/saturnmap.jpg"),
      rotation: 0.005,
      bumpMap: useLoader(TextureLoader, "/assets/saturnmap.jpg"),
      bumpScale: 0.05,
      specularMap: undefined,
      specular: undefined,
    },
    "!uranus": {
      texture: useLoader(TextureLoader, "/assets/uranusmap.jpg"),
      rotation: 0.003,
      bumpMap: useLoader(TextureLoader, "/assets/uranusmap.jpg"),
      bumpScale: 0.05,
      specularMap: undefined,
      specular: undefined,
    },
    "!neptune": {
      texture: useLoader(TextureLoader, "/assets/neptunemap.jpg"),
      rotation: 0.003,
      bumpMap: useLoader(TextureLoader, "/assets/neptunemap.jpg"),
      bumpScale: 0.05,
      specularMap: undefined,
      specular: undefined,
    },
    "!pluto": {
      texture: useLoader(TextureLoader, "/assets/plutomap1k.jpg"),
      rotation: 0.001,
      bumpMap: useLoader(TextureLoader, "/assets/plutobump1k.jpg"),
      bumpScale: 0.005,
      specularMap: undefined,
      specular: undefined,
    },
  };

  let texture_details = PLANET_TEMPLATES[args.planet.color];

  const ref = useRef<THREE.Mesh>(null);
  useFrame(() => {
    if (texture_details != null && ref.current != null) {
      ref.current.rotation.y += texture_details.rotation;
    }
  });

  if (texture_details != null) {
    return (
      <>
      {args.planet.gravity_radius_025 && gravityWell(args.planet.gravity_radius_025, 5)}
      {args.planet.gravity_radius_05 && gravityWell(args.planet.gravity_radius_05, 4)}
      {args.planet.gravity_radius_1 && gravityWell(args.planet.gravity_radius_1, 3)}
      {args.planet.gravity_radius_2 && gravityWell(args.planet.gravity_radius_2, 2 )}
      <mesh
        ref={ref}
        rotation-y={1}
        position={pos}
        onPointerOver={() => entityToShow.setEntityToShow(args.planet)}
        onPointerLeave={() => entityToShow.setEntityToShow(null)}>
        <icosahedronGeometry args={[radiusUnits, 15]} />
        <meshPhongMaterial
          map={texture_details.texture}
          bumpMap={texture_details.bumpMap}
          bumpScale={texture_details.bumpScale}
          specularMap={texture_details.specularMap}
          specular={texture_details.specular}
          shininess={8}
          side={THREE.FrontSide}
          transparent={false}
        />
      </mesh>
      </>
    );
  } else {
    const color = Color(args.planet.color);
    const GLOW_INTENSITY = 2.5;

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
              (color.red() / 255.0) * GLOW_INTENSITY,
              (color.green() / 255.0) * GLOW_INTENSITY,
              (color.blue() / 255.0) * GLOW_INTENSITY,
            ]}
          />
        </mesh>
      </>
    );
  }
}

function Planets(args: { planets: PlanetType[], controlGravityWell: boolean }) {
  return (
    <>
      {args.planets.map((planet, index) => (
        <Planet key={planet.name} planet={planet} controlGravityWell={args.controlGravityWell} />
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

function SpaceView(args: { controlGravityWell: boolean }) {
  const serverEntities = useContext(EntitiesServerContext);
  const planets = serverEntities.entities.planets;

  return (
    <>
      <Axes />
      <Planets planets={planets} controlGravityWell={args.controlGravityWell} />
      <Galaxy />
    </>
  );
}

export default SpaceView;
