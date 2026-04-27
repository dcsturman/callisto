import * as THREE from "three";

export type LoadedPlanetTextures = {
  texture: THREE.Texture;
  bumpMap: THREE.Texture | undefined;
  specularMap: THREE.Texture | undefined;
  rotation: number;
  bumpScale: number;
  specular: THREE.Color | undefined;
};

// Cache for loaded textures
export const textureCache = new Map<string, LoadedPlanetTextures>();

// Texture definitions for lazy loading
export const PLANET_TEXTURE_DEFINITIONS = {
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

