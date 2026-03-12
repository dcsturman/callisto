import * as React from "react";
import { useRef, useMemo } from "react";
import * as THREE from "three";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { KernelSize, Resolution } from "postprocessing";
import { TextureLoader } from "three/src/loaders/TextureLoader";
import { useLoader, useFrame } from "@react-three/fiber";

import Color from "color";

import { Line, scaleVector } from "lib/Util";
import { SCALE } from "lib/universal";
import { Planet as PlanetType } from "lib/entities";

import { RangeSphere } from "lib/Util";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { setEntityToShow } from "state/uiSlice";
import { entitiesSelector } from "state/serverSlice";

// Texture definitions for lazy loading
const PLANET_TEXTURE_DEFINITIONS = {
  "!earth": {
    texture: "/assets/earthmap1k.jpg",
    bumpMap: "/assets/earthbump1k.jpg",
    specularMap: "/assets/earthspec1k.jpg",
    rotation: 0.002,
    bumpScale: 0.05,
    specular: new THREE.Color("grey"),
  },
  "!sun": {
    texture: "/assets/sunmap.jpg",
    bumpMap: "/assets/sunmap.jpg",
    specularMap: undefined,
    rotation: 0.0,
    bumpScale: 0.05,
    specular: undefined,
  },
  "!moon": {
    texture: "/assets/moonmap1k.jpg",
    bumpMap: "/assets/moonbump1k.jpg",
    specularMap: undefined,
    rotation: 0.0,
    bumpScale: 0.002,
    specular: undefined,
  },
  "!mercury": {
    texture: "/assets/mercurymap.jpg",
    bumpMap: "/assets/mercurybump.jpg",
    specularMap: undefined,
    rotation: 0.0,
    bumpScale: 0.005,
    specular: undefined,
  },
  "!venus": {
    texture: "/assets/venusmap.jpg",
    bumpMap: "/assets/venusbump.jpg",
    specularMap: undefined,
    rotation: 0.0005,
    bumpScale: 0.005,
    specular: undefined,
  },
  "!mars": {
    texture: "/assets/marsmap1k.jpg",
    bumpMap: "/assets/marsbump1k.jpg",
    specularMap: undefined,
    rotation: 0.002,
    bumpScale: 0.05,
    specular: undefined,
  },
  "!jupiter": {
    texture: "/assets/jupitermap.jpg",
    bumpMap: "/assets/jupitermap.jpg",
    specularMap: undefined,
    rotation: 0.005,
    bumpScale: 0.02,
    specular: undefined,
  },
  "!saturn": {
    texture: "/assets/saturnmap.jpg",
    bumpMap: "/assets/saturnmap.jpg",
    specularMap: undefined,
    rotation: 0.005,
    bumpScale: 0.05,
    specular: undefined,
  },
  "!uranus": {
    texture: "/assets/uranusmap.jpg",
    bumpMap: "/assets/uranusmap.jpg",
    specularMap: undefined,
    rotation: 0.003,
    bumpScale: 0.05,
    specular: undefined,
  },
  "!neptune": {
    texture: "/assets/neptunemap.jpg",
    bumpMap: "/assets/neptunemap.jpg",
    specularMap: undefined,
    rotation: 0.003,
    bumpScale: 0.05,
    specular: undefined,
  },
  "!pluto": {
    texture: "/assets/plutomap1k.jpg",
    bumpMap: "/assets/plutobump1k.jpg",
    specularMap: undefined,
    rotation: 0.001,
    bumpScale: 0.005,
    specular: undefined,
  },
} as const;

// Cache for loaded textures
type LoadedPlanetTextures = {
  texture: THREE.Texture;
  bumpMap: THREE.Texture | undefined;
  specularMap: THREE.Texture | undefined;
  rotation: number;
  bumpScale: number;
  specular: THREE.Color | undefined;
};

const textureCache = new Map<string, LoadedPlanetTextures>();

function Planet(args: {
  planet: PlanetType;
  controlGravityWell: boolean;
  controlJumpDistance: boolean;
}) {
  const dispatch = useAppDispatch();

  const radiusMeters = args.planet.radius;
  const radiusUnits = radiusMeters * SCALE;
  const pos = scaleVector(args.planet.position, SCALE);

  function allViewChanges() {
    return (
      <>
        {args.controlJumpDistance && (
          <mesh position={pos} renderOrder={12}>
            <sphereGeometry args={[args.planet.radius * 200 * SCALE, 14, 14]} />
            <meshBasicMaterial
              color="#888888"
              wireframe={true}
              alphaToCoverage={false}
              transparent={true}
            />
          </mesh>
        )}
        {args.controlGravityWell && args.planet.gravity_radius_025 && (
          <RangeSphere
            pos={pos}
            distance={args.planet.gravity_radius_025}
            order={10}
          />
        )}
        {args.controlGravityWell && args.planet.gravity_radius_05 && (
          <RangeSphere
            pos={pos}
            distance={args.planet.gravity_radius_05}
            order={8}
          />
        )}
        {args.controlGravityWell && args.planet.gravity_radius_1 && (
          <RangeSphere
            pos={pos}
            distance={args.planet.gravity_radius_1}
            order={6}
          />
        )}
        {args.controlGravityWell && args.planet.gravity_radius_2 && (
          <RangeSphere
            pos={pos}
            distance={args.planet.gravity_radius_2}
            order={4}
          />
        )}
      </>
    );
  }

  // Lazy load textures only when needed
  const texture_details = useMemo(() => {
    const planetDef =
      PLANET_TEXTURE_DEFINITIONS[
        args.planet.color as keyof typeof PLANET_TEXTURE_DEFINITIONS
      ];
    if (!planetDef) return null;

    // Check cache first
    const cacheKey = args.planet.color;
    if (textureCache.has(cacheKey)) {
      return textureCache.get(cacheKey);
    }

    // Load textures on demand
    const textureLoader = new TextureLoader();
    const texture = textureLoader.load(planetDef.texture);
    const bumpMap = planetDef.bumpMap
      ? textureLoader.load(planetDef.bumpMap)
      : undefined;
    const specularMap = planetDef.specularMap
      ? textureLoader.load(planetDef.specularMap)
      : undefined;

    const loaded = {
      texture,
      bumpMap,
      specularMap,
      rotation: planetDef.rotation,
      bumpScale: planetDef.bumpScale,
      specular: planetDef.specular,
    };

    // Cache the loaded textures
    textureCache.set(cacheKey, loaded);
    return loaded;
  }, [args.planet.color]);

  const ref = useRef<THREE.Mesh>(null);
  useFrame(() => {
    if (texture_details != null && ref.current != null) {
      ref.current.rotation.y += texture_details.rotation;
    }
  });

  if (texture_details != null) {
    return (
      <>
        {allViewChanges()}
        <mesh
          ref={ref}
          rotation-y={1}
          position={pos}
          onPointerOver={() => dispatch(setEntityToShow(args.planet))}
          onPointerLeave={() => dispatch(setEntityToShow(null))}
        >
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
        {allViewChanges()}
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
          onPointerOver={() => dispatch(setEntityToShow(args.planet))}
          onPointerLeave={() => dispatch(setEntityToShow(null))}
        >
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
      <Galaxy />
    </>
  );
}

export default SpaceView;
