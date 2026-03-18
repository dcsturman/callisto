import * as THREE from "three";

// Shader for procedural noise texture
const NOISE_TEXTURE_VERTEX = `
  varying vec3 vPosition;
  varying vec3 vNormal;

  void main() {
    vPosition = position;
    vNormal = normalize(normalMatrix * normal);
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const NOISE_TEXTURE_FRAGMENT = `
  varying vec3 vPosition;
  varying vec3 vNormal;
  uniform vec3 baseColor;

  // Simple hash-based noise function
  float hash(vec3 p) {
    return fract(sin(dot(p, vec3(12.9898, 78.233, 45.164))) * 43758.5453);
  }

  float noise(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);

    float n000 = hash(i + vec3(0.0, 0.0, 0.0));
    float n100 = hash(i + vec3(1.0, 0.0, 0.0));
    float n010 = hash(i + vec3(0.0, 1.0, 0.0));
    float n110 = hash(i + vec3(1.0, 1.0, 0.0));
    float n001 = hash(i + vec3(0.0, 0.0, 1.0));
    float n101 = hash(i + vec3(1.0, 0.0, 1.0));
    float n011 = hash(i + vec3(0.0, 1.0, 1.0));
    float n111 = hash(i + vec3(1.0, 1.0, 1.0));

    float nx0 = mix(n000, n100, f.x);
    float nx1 = mix(n010, n110, f.x);
    float nxy0 = mix(nx0, nx1, f.y);

    float nx0z = mix(n001, n101, f.x);
    float nx1z = mix(n011, n111, f.x);
    float nxy1 = mix(nx0z, nx1z, f.y);

    return mix(nxy0, nxy1, f.z);
  }

  void main() {
    float n = noise(vPosition * 5.0);
    vec3 color = baseColor * (0.7 + 0.3 * n);
    gl_FragColor = vec4(color, 1.0);
  }
`;

// Shader for striped bands (gas giants)
const STRIPED_BANDS_VERTEX = `
  varying vec3 vPosition;

  void main() {
    vPosition = position;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const STRIPED_BANDS_FRAGMENT = `
  varying vec3 vPosition;
  uniform vec3 baseColor;

  float hash(float x) {
    return fract(sin(x * 12.9898) * 43758.5453);
  }

  void main() {
    float bands = sin(vPosition.y * 10.0) * 0.5 + 0.5;
    float noise = hash(vPosition.y * 20.0) * 0.2;
    float intensity = bands * 0.6 + noise * 0.4;
    vec3 color = baseColor * (0.8 + 0.2 * intensity);
    gl_FragColor = vec4(color, 1.0);
  }
`;

// Shader for latitude-based color variation
const LATITUDE_COLOR_VERTEX = `
  varying vec3 vPosition;
  varying vec3 vNormal;

  void main() {
    vPosition = position;
    vNormal = normalize(normalMatrix * normal);
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const LATITUDE_COLOR_FRAGMENT = `
  varying vec3 vPosition;
  varying vec3 vNormal;
  uniform vec3 baseColor;

  float hash(float x) {
    return fract(sin(x * 12.9898) * 43758.5453);
  }

  void main() {
    float latitude = vPosition.y;
    float variation = hash(latitude * 5.0) * 0.3;
    float intensity = abs(latitude) * 0.5 + variation;
    vec3 color = baseColor * (0.7 + 0.3 * intensity);
    gl_FragColor = vec4(color, 1.0);
  }
`;

// Shader for animated clouds
const ANIMATED_CLOUDS_VERTEX = `
  varying vec3 vSurfaceNormal;

  void main() {
    vSurfaceNormal = normalize(position);
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const ANIMATED_CLOUDS_FRAGMENT = `
  varying vec3 vSurfaceNormal;
  uniform vec3 baseColor;
  uniform float time;

  float hash(vec3 p) {
    return fract(sin(dot(p, vec3(127.1, 311.7, 191.9))) * 43758.5453123);
  }

  float noise(vec3 p) {
    vec3 i = floor(p);
    vec3 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);

    float n000 = hash(i + vec3(0.0, 0.0, 0.0));
    float n100 = hash(i + vec3(1.0, 0.0, 0.0));
    float n010 = hash(i + vec3(0.0, 1.0, 0.0));
    float n110 = hash(i + vec3(1.0, 1.0, 0.0));
    float n001 = hash(i + vec3(0.0, 0.0, 1.0));
    float n101 = hash(i + vec3(1.0, 0.0, 1.0));
    float n011 = hash(i + vec3(0.0, 1.0, 1.0));
    float n111 = hash(i + vec3(1.0, 1.0, 1.0));

    float nx00 = mix(n000, n100, f.x);
    float nx10 = mix(n010, n110, f.x);
    float nx01 = mix(n001, n101, f.x);
    float nx11 = mix(n011, n111, f.x);

    float nxy0 = mix(nx00, nx10, f.y);
    float nxy1 = mix(nx01, nx11, f.y);
    return mix(nxy0, nxy1, f.z);
  }

  float fbm(vec3 p) {
    float value = 0.0;
    float amplitude = 0.5;
    for (int i = 0; i < 4; i++) {
      value += amplitude * noise(p);
      p *= 2.0;
      amplitude *= 0.5;
    }
    return value;
  }

  vec3 rotateY(vec3 p, float angle) {
    float c = cos(angle);
    float s = sin(angle);
    return vec3(c * p.x + s * p.z, p.y, -s * p.x + c * p.z);
  }

  void main() {
    vec3 n = normalize(vSurfaceNormal);
    vec3 rotated = rotateY(n, time * 0.18);
    float cloud = fbm(rotated * 3.4 + vec3(0.0, 0.0, 0.0));
    cloud = cloud * 0.7 + fbm(rotated * 7.1 + vec3(4.3, -2.1, 1.7)) * 0.3;
    cloud = smoothstep(0.5, 0.74, cloud);

    float light = dot(n, normalize(vec3(0.4, 0.3, 1.0))) * 0.35 + 0.65;
    vec3 cloudColor = mix(baseColor, vec3(1.0), 0.72);
    vec3 color = cloudColor * light;
    float alpha = cloud * 0.72;
    color *= light;

    gl_FragColor = vec4(color, alpha);
  }
`;

const PLANETARY_RING_VERTEX = `
  varying vec3 vPosition;

  void main() {
    vPosition = position;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

const PLANETARY_RING_FRAGMENT = `
  varying vec3 vPosition;
  uniform vec3 baseColor;
  uniform float innerRadius;
  uniform float outerRadius;

  float hash(float x) {
    return fract(sin(x * 91.3458) * 47453.5453);
  }

  void main() {
    float radius = length(vPosition.xy);
    float radialT = clamp((radius - innerRadius) / (outerRadius - innerRadius), 0.0, 1.0);

    float bandA = 0.5 + 0.5 * sin(radialT * 45.0);
    float bandB = 0.5 + 0.5 * sin(radialT * 120.0 + 1.3);
    float grain = hash(floor(radialT * 160.0)) * 0.25;
    float bandMix = bandA * 0.55 + bandB * 0.25 + grain;

    float alpha = smoothstep(0.02, 0.12, radialT) * (1.0 - smoothstep(0.82, 0.98, radialT));
    alpha *= 0.18 + 0.5 * bandMix;

    vec3 ringTint = mix(baseColor * 0.8, min(baseColor + vec3(0.45), vec3(1.0)), 0.55 + 0.25 * bandMix);
    gl_FragColor = vec4(ringTint, alpha);
  }
`;

type GeneratedSurfaceMaps = {
  map: THREE.DataTexture;
  bumpMap: THREE.DataTexture;
};

const continentsSurfaceCache = new Map<string, GeneratedSurfaceMaps>();

function hashString(value: string): number {
  let hash = 2166136261;

  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }

  return hash >>> 0;
}

function fract(value: number): number {
  return value - Math.floor(value);
}

function lerp(a: number, b: number, t: number): number {
  return a + (b - a) * t;
}

function smoothstep(edge0: number, edge1: number, value: number): number {
  const t = THREE.MathUtils.clamp((value - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
}

function noiseHash3(x: number, y: number, z: number, seed: number): number {
  return fract(
    Math.sin(x * 127.1 + y * 311.7 + z * 191.9 + seed * 0.00137) * 43758.5453123,
  );
}

function valueNoise3(x: number, y: number, z: number, seed: number): number {
  const xi = Math.floor(x);
  const yi = Math.floor(y);
  const zi = Math.floor(z);
  const xf = x - xi;
  const yf = y - yi;
  const zf = z - zi;

  const sx = xf * xf * (3 - 2 * xf);
  const sy = yf * yf * (3 - 2 * yf);
  const sz = zf * zf * (3 - 2 * zf);

  const n000 = noiseHash3(xi, yi, zi, seed);
  const n100 = noiseHash3(xi + 1, yi, zi, seed);
  const n010 = noiseHash3(xi, yi + 1, zi, seed);
  const n110 = noiseHash3(xi + 1, yi + 1, zi, seed);
  const n001 = noiseHash3(xi, yi, zi + 1, seed);
  const n101 = noiseHash3(xi + 1, yi, zi + 1, seed);
  const n011 = noiseHash3(xi, yi + 1, zi + 1, seed);
  const n111 = noiseHash3(xi + 1, yi + 1, zi + 1, seed);

  const nx00 = lerp(n000, n100, sx);
  const nx10 = lerp(n010, n110, sx);
  const nx01 = lerp(n001, n101, sx);
  const nx11 = lerp(n011, n111, sx);

  const nxy0 = lerp(nx00, nx10, sy);
  const nxy1 = lerp(nx01, nx11, sy);

  return lerp(nxy0, nxy1, sz);
}

function fbm3(x: number, y: number, z: number, seed: number, octaves = 5): number {
  let total = 0;
  let amplitude = 0.5;
  let frequency = 1;

  for (let i = 0; i < octaves; i += 1) {
    total += amplitude * valueNoise3(x * frequency, y * frequency, z * frequency, seed + i * 97);
    frequency *= 2;
    amplitude *= 0.5;
  }

  return total;
}

function buildContinentsSurface(
  planetName: string,
  baseColor: THREE.Color,
): GeneratedSurfaceMaps {
  const cacheKey = `${planetName}:${baseColor.getHexString()}`;
  const cached = continentsSurfaceCache.get(cacheKey);

  if (cached) {
    return cached;
  }

  const width = 256;
  const height = 128;
  const colorData = new Uint8Array(width * height * 4);
  const bumpData = new Uint8Array(width * height * 4);
  const seed = hashString(cacheKey);

  const deepOcean = baseColor.clone().lerp(new THREE.Color("#0d274f"), 0.55).multiplyScalar(0.72);
  const shallowOcean = baseColor.clone().lerp(new THREE.Color("#2f78b8"), 0.3);
  const beach = new THREE.Color("#d4c28d");
  const lowland = new THREE.Color("#4f8f42");
  const dryland = new THREE.Color("#8f7442");
  const highland = new THREE.Color("#7c6942");
  const mountain = new THREE.Color("#b4ab97");
  const snow = new THREE.Color("#f2f5f8");

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const u = x === width - 1 ? 0 : x / (width - 1);
      const v = y / (height - 1);
      const latitude = Math.abs(v * 2 - 1);
      const longitude = u * Math.PI * 2;
      const latitudeAngle = (v - 0.5) * Math.PI;
      const cosLat = Math.cos(latitudeAngle);
      const dirX = Math.cos(longitude) * cosLat;
      const dirY = Math.sin(latitudeAngle);
      const dirZ = Math.sin(longitude) * cosLat;

      const warpX = fbm3(dirX * 1.4 + 3.7, dirY * 1.4 - 2.1, dirZ * 1.4 + 0.9, seed + 17, 3) - 0.5;
      const warpY = fbm3(dirX * 1.4 - 4.3, dirY * 1.4 + 1.2, dirZ * 1.4 - 1.6, seed + 41, 3) - 0.5;
      const warpZ = fbm3(dirX * 1.4 + 2.6, dirY * 1.4 - 3.1, dirZ * 1.4 + 4.4, seed + 59, 3) - 0.5;

      const sampleX = dirX * 2.2 + warpX * 0.7;
      const sampleY = dirY * 2.2 + warpY * 0.5;
      const sampleZ = dirZ * 2.2 + warpZ * 0.7;

      const continental = fbm3(sampleX, sampleY, sampleZ, seed + 101, 5);
      const detail = fbm3(sampleX * 2.8, sampleY * 2.8, sampleZ * 2.8, seed + 211, 4);
      const ridge =
        1 - Math.abs(2 * fbm3(sampleX * 2.1, sampleY * 2.1, sampleZ * 2.1, seed + 307, 3) - 1);
      const elevation = continental * 0.74 + detail * 0.16 + ridge * 0.1 - latitude * 0.06;

      const shoreline = smoothstep(0.45, 0.58, elevation);
      const landMask = smoothstep(0.52, 0.61, elevation);
      const mountainMask = smoothstep(0.68, 0.84, elevation);
      const polarMask = smoothstep(0.8, 0.96, latitude) * smoothstep(0.56, 0.76, elevation);
      const vegetation = THREE.MathUtils.clamp(1.05 - latitude * 1.4 + detail * 0.35, 0, 1);
      const aridityNoise = fbm3(sampleX * 3.6 + 7.7, sampleY * 3.6 - 1.8, sampleZ * 3.6 + 2.4, seed + 401, 4);
      const aridityBand = 1 - smoothstep(0.52, 0.95, latitude);
      const dryMask = landMask * smoothstep(0.54, 0.7, aridityNoise + aridityBand * 0.22 - vegetation * 0.18);

      const oceanColor = deepOcean.clone().lerp(shallowOcean, shoreline * 0.9);
      const landColor = highland.clone().lerp(lowland, vegetation);
      landColor.lerp(dryland, dryMask * 0.9);
      landColor.lerp(mountain, mountainMask);

      const surfaceColor = oceanColor.clone().lerp(beach, shoreline);
      surfaceColor.lerp(landColor, landMask);
      surfaceColor.lerp(snow, polarMask);

      const bumpValue = THREE.MathUtils.clamp(
        Math.round((shoreline * 0.35 + landMask * 0.3 + mountainMask * 0.35) * 255),
        0,
        255,
      );

      const index = (y * width + x) * 4;
      colorData[index] = Math.round(surfaceColor.r * 255);
      colorData[index + 1] = Math.round(surfaceColor.g * 255);
      colorData[index + 2] = Math.round(surfaceColor.b * 255);
      colorData[index + 3] = 255;

      bumpData[index] = bumpValue;
      bumpData[index + 1] = bumpValue;
      bumpData[index + 2] = bumpValue;
      bumpData[index + 3] = 255;
    }
  }

  const map = new THREE.DataTexture(colorData, width, height, THREE.RGBAFormat);
  map.colorSpace = THREE.SRGBColorSpace;
  map.wrapS = THREE.RepeatWrapping;
  map.wrapT = THREE.ClampToEdgeWrapping;
  map.magFilter = THREE.LinearFilter;
  map.minFilter = THREE.LinearMipmapLinearFilter;
  map.needsUpdate = true;

  const bumpMap = new THREE.DataTexture(bumpData, width, height, THREE.RGBAFormat);
  bumpMap.wrapS = THREE.RepeatWrapping;
  bumpMap.wrapT = THREE.ClampToEdgeWrapping;
  bumpMap.magFilter = THREE.LinearFilter;
  bumpMap.minFilter = THREE.LinearMipmapLinearFilter;
  bumpMap.needsUpdate = true;

  const generated = { map, bumpMap };
  continentsSurfaceCache.set(cacheKey, generated);
  return generated;
}

export interface ShaderMaterialProps {
  baseColor: THREE.Color;
  time?: number;
}

export function createContinentsMaterial(
  planetName: string,
  baseColor: THREE.Color,
): THREE.MeshPhongMaterial {
  const { map, bumpMap } = buildContinentsSurface(planetName, baseColor);

  return new THREE.MeshPhongMaterial({
    color: new THREE.Color("white"),
    map,
    bumpMap,
    bumpScale: 0.035,
    shininess: 8,
    specular: new THREE.Color("#52634a"),
  });
}

export function createNoiseTextureMaterial(
  baseColor: THREE.Color,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: NOISE_TEXTURE_VERTEX,
    fragmentShader: NOISE_TEXTURE_FRAGMENT,
    uniforms: {
      baseColor: { value: baseColor },
    },
  });
}

export function createStripedBandsMaterial(
  baseColor: THREE.Color,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: STRIPED_BANDS_VERTEX,
    fragmentShader: STRIPED_BANDS_FRAGMENT,
    uniforms: {
      baseColor: { value: baseColor },
    },
  });
}

export function createLatitudeColorMaterial(
  baseColor: THREE.Color,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: LATITUDE_COLOR_VERTEX,
    fragmentShader: LATITUDE_COLOR_FRAGMENT,
    uniforms: {
      baseColor: { value: baseColor },
    },
  });
}

export function createAnimatedCloudsMaterial(
  baseColor: THREE.Color,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: ANIMATED_CLOUDS_VERTEX,
    fragmentShader: ANIMATED_CLOUDS_FRAGMENT,
    transparent: true,
    depthWrite: false,
    side: THREE.FrontSide,
    uniforms: {
      baseColor: { value: baseColor },
      time: { value: 0 },
    },
  });
}

export function createPlanetaryRingMaterial(
  baseColor: THREE.Color,
  innerRadius: number,
  outerRadius: number,
): THREE.ShaderMaterial {
  return new THREE.ShaderMaterial({
    vertexShader: PLANETARY_RING_VERTEX,
    fragmentShader: PLANETARY_RING_FRAGMENT,
    transparent: true,
    side: THREE.DoubleSide,
    depthWrite: false,
    uniforms: {
      baseColor: { value: baseColor },
      innerRadius: { value: innerRadius },
      outerRadius: { value: outerRadius },
    },
  });
}
