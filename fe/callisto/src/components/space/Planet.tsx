import React, { useRef, useMemo } from "react";
import * as THREE from "three";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { KernelSize, Resolution } from "postprocessing";
import { TextureLoader } from "three/src/loaders/TextureLoader";
import { useFrame } from "@react-three/fiber";

import { scaleVector } from "lib/Util";
import { SCALE } from "lib/universal";
import {
  Planet as PlanetType,
  PlanetVisualEffect,
  effectsToBitmask,
  hasEffect,
} from "lib/entities";

import { RangeSphere } from "lib/Util";

import { useAppDispatch } from "state/hooks";
import { setEntityToShow } from "state/uiSlice";

import {
  createContinentsMaterial,
  createNoiseTextureMaterial,
  createStripedBandsMaterial,
  createLatitudeColorMaterial,
  createAnimatedCloudsMaterial,
  createPlanetaryRingMaterial,
} from "./PlanetVisualEffects";

import {
  PLANET_TEXTURE_DEFINITIONS,
  LoadedPlanetTextures,
  textureCache,
} from "./PlanetTextures";

interface PlanetProps {
  planet: PlanetType;
  controlGravityWell: boolean;
  controlJumpDistance: boolean;
}

export function Planet(args: PlanetProps) {
  const dispatch = useAppDispatch();

  const radiusMeters = args.planet.radius;
  const radiusUnits = radiusMeters * SCALE;
  const pos = scaleVector(args.planet.position, SCALE);
  const effectColor = useMemo(() => {
    if (args.planet.color.startsWith("!")) {
      return new THREE.Color("#d8c7a3");
    }

    return new THREE.Color(args.planet.color);
  }, [args.planet.color]);
  const effectsBitmask = effectsToBitmask(args.planet.visual_effects);
  const useAtmosphereRing = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.ATMOSPHERE_RING,
  );
  const usePlanetaryRing = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.PLANETARY_RING,
  );
  const useAnimatedClouds = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.ANIMATED_CLOUDS,
  );

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

    const cacheKey = args.planet.color;
    if (textureCache.has(cacheKey)) {
      return textureCache.get(cacheKey);
    }

    const textureLoader = new TextureLoader();
    const texture = textureLoader.load(planetDef.texture);
    const bumpMap = planetDef.bumpMap
      ? textureLoader.load(planetDef.bumpMap)
      : undefined;
    const specularMap = planetDef.specularMap
      ? textureLoader.load(planetDef.specularMap)
      : undefined;

    const loaded: LoadedPlanetTextures = {
      texture,
      bumpMap,
      specularMap,
      rotation: planetDef.rotation,
      bumpScale: planetDef.bumpScale,
      specular: planetDef.specular,
    };

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
        <PlanetEffectOverlays
          pos={pos}
          radiusUnits={radiusUnits}
          color={effectColor}
          useAtmosphereRing={useAtmosphereRing}
          usePlanetaryRing={usePlanetaryRing}
          useAnimatedClouds={useAnimatedClouds}
        />
      </>
    );
  } else {
    return (
      <ProceduralPlanet
        planet={args.planet}
        pos={pos}
        radiusUnits={radiusUnits}
        effectColor={effectColor}
        useAtmosphereRing={useAtmosphereRing}
        usePlanetaryRing={usePlanetaryRing}
        useAnimatedClouds={useAnimatedClouds}
        allViewChanges={allViewChanges}
      />
    );
  }
}

interface ProceduralPlanetProps {
  planet: PlanetType;
  pos: [number, number, number];
  radiusUnits: number;
  effectColor: THREE.Color;
  useAtmosphereRing: boolean;
  usePlanetaryRing: boolean;
  useAnimatedClouds: boolean;
  allViewChanges: () => React.JSX.Element;
}

function ProceduralPlanet({
  planet,
  pos,
  radiusUnits,
  effectColor,
  useAtmosphereRing,
  usePlanetaryRing,
  useAnimatedClouds,
  allViewChanges,
}: ProceduralPlanetProps) {
  const dispatch = useAppDispatch();
  const effectsBitmask = effectsToBitmask(planet.visual_effects);
  const usePhongLighting = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.PHONG_LIGHTING,
  );
  const useNoiseTexture = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.NOISE_TEXTURE,
  );
  const useContinents = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.CONTINENTS,
  );
  const useStripedBands = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.STRIPED_BANDS,
  );
  const useLatitudeColor = hasEffect(
    effectsBitmask,
    PlanetVisualEffect.LATITUDE_COLOR,
  );
  const material = useMemo(() => {
    if (useContinents) {
      return createContinentsMaterial(planet.name, effectColor);
    }
    if (useStripedBands) return createStripedBandsMaterial(effectColor);
    if (useLatitudeColor) return createLatitudeColorMaterial(effectColor);
    if (useNoiseTexture) return createNoiseTextureMaterial(effectColor);
    if (usePhongLighting)
      return new THREE.MeshPhongMaterial({ color: effectColor });
    return new THREE.MeshBasicMaterial({ color: effectColor });
  }, [
    planet.name,
    effectColor,
    useContinents,
    useStripedBands,
    useLatitudeColor,
    useNoiseTexture,
    usePhongLighting,
  ]);

  return (
    <>
      {allViewChanges()}
      {useAnimatedClouds && (
        <EffectComposer>
          <Bloom
            intensity={0.8}
            blurPass={undefined}
            kernelSize={KernelSize.LARGE}
            luminanceThreshold={0.9}
            luminanceSmoothing={0.025}
            mipmapBlur={true}
            resolutionX={Resolution.AUTO_SIZE}
            resolutionY={Resolution.AUTO_SIZE}
          />
        </EffectComposer>
      )}
      <mesh
        position={pos}
        onPointerOver={() => dispatch(setEntityToShow(planet))}
        onPointerLeave={() => dispatch(setEntityToShow(null))}
      >
        {useContinents ? (
          <sphereGeometry args={[radiusUnits, 64, 32]} />
        ) : (
          <icosahedronGeometry args={[radiusUnits, 15]} />
        )}
        <primitive object={material} attach="material" />
      </mesh>
      <PlanetEffectOverlays
        pos={pos}
        radiusUnits={radiusUnits}
        color={effectColor}
        useAtmosphereRing={useAtmosphereRing}
        usePlanetaryRing={usePlanetaryRing}
        useAnimatedClouds={useAnimatedClouds}
      />
    </>
  );
}

interface PlanetEffectOverlaysProps {
  pos: [number, number, number];
  radiusUnits: number;
  color: THREE.Color;
  useAtmosphereRing: boolean;
  usePlanetaryRing: boolean;
  useAnimatedClouds: boolean;
}

function PlanetEffectOverlays({
  pos,
  radiusUnits,
  color,
  useAtmosphereRing,
  usePlanetaryRing,
  useAnimatedClouds,
}: PlanetEffectOverlaysProps) {
  const ringInnerRadius = radiusUnits * 1.35;
  const ringOuterRadius = radiusUnits * 2.2;
  const ringMaterial = useMemo(
    () => createPlanetaryRingMaterial(color, ringInnerRadius, ringOuterRadius),
    [color, ringInnerRadius, ringOuterRadius],
  );
  const cloudMaterial = useMemo(() => createAnimatedCloudsMaterial(color), [color]);

  useFrame(({ clock }) => {
    if (useAnimatedClouds) {
      cloudMaterial.uniforms.time.value = clock.getElapsedTime();
    }
  });

  return (
    <>
      {useAtmosphereRing && (
        <mesh position={pos}>
          <sphereGeometry args={[radiusUnits * 1.1, 32, 32]} />
          <meshBasicMaterial
            color={color}
            transparent={true}
            opacity={0.2}
            side={THREE.BackSide}
          />
        </mesh>
      )}
      {usePlanetaryRing && (
        <mesh position={pos} rotation={[Math.PI / 3.5, 0, 0.25]} material={ringMaterial}>
          <ringGeometry args={[ringInnerRadius, ringOuterRadius, 96]} />
        </mesh>
      )}
      {useAnimatedClouds && (
        <mesh position={pos} material={cloudMaterial} renderOrder={2}>
          <sphereGeometry args={[radiusUnits * 1.032, 48, 24]} />
        </mesh>
      )}
    </>
  );
}
