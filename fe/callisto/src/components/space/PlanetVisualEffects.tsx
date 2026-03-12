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

  const float PI = 3.141592653589793;

  float hash(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
  }

  float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);

    float n00 = hash(i + vec2(0.0, 0.0));
    float n10 = hash(i + vec2(1.0, 0.0));
    float n01 = hash(i + vec2(0.0, 1.0));
    float n11 = hash(i + vec2(1.0, 1.0));

    float nx0 = mix(n00, n10, f.x);
    float nx1 = mix(n01, n11, f.x);
    return mix(nx0, nx1, f.y);
  }

  float fbm(vec2 p) {
    float value = 0.0;
    float amplitude = 0.5;
    for (int i = 0; i < 4; i++) {
      value += amplitude * noise(p);
      p *= 2.0;
      amplitude *= 0.5;
    }
    return value;
  }

  void main() {
    vec3 n = normalize(vSurfaceNormal);
    float longitude = atan(n.z, n.x) / (2.0 * PI) + 0.5;
    float latitude = asin(clamp(n.y, -1.0, 1.0)) / PI + 0.5;

    vec2 uv = vec2(longitude + time * 0.02, latitude);
    float cloud = fbm(uv * vec2(8.0, 4.0));
    cloud = smoothstep(0.45, 0.72, cloud);

    float light = dot(n, normalize(vec3(0.4, 0.3, 1.0))) * 0.35 + 0.65;
    vec3 cloudColor = min(baseColor + vec3(0.35), vec3(1.0));
    vec3 color = mix(baseColor * 0.8, cloudColor, cloud * 0.85);
    color *= light;

    gl_FragColor = vec4(color, 1.0);
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

export interface ShaderMaterialProps {
  baseColor: THREE.Color;
  time?: number;
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
