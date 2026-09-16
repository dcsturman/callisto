import * as React from "react";
import * as THREE from "three";
import { useLayoutEffect, useRef } from "react";
import { extend, useFrame, useThree } from "@react-three/fiber";
import { Text } from "@react-three/drei";
import { SCALE, RANGE_BANDS } from "./universal";
import { sphereSilhouette } from "./range";

extend({ Line_: THREE.Line });

/* Entities come in from the server with all units in meters (m). Convert them to the units we can use on screen. */
export function scaleVector(
  v: [number, number, number],
  scale: number,
): [number, number, number] {
  return v.map((x) => x * scale) as [number, number, number];
}

export function addVector(
  a: [number, number, number],
  b: [number, number, number],
) {
  return a.map((x, i) => x + b[i]) as [number, number, number];
}

export function vectorToString(
  v: [number, number, number],
  precision: number = 0,
) {
  return `${v[0].toFixed(precision)}, ${v[1].toFixed(precision)}, ${v[2].toFixed(precision)}`;
}

export function vectorDistance(
  a: [number, number, number],
  b: [number, number, number],
) {
  return Math.sqrt(
    (a[0] - b[0]) * (a[0] - b[0]) +
      (a[1] - b[1]) * (a[1] - b[1]) +
      (a[2] - b[2]) * (a[2] - b[2]),
  );
}

export function Line({
  start,
  end,
  color = "grey",
  scale = 1.0,
  debug = false,
}: {
  start: [number, number, number];
  end: [number, number, number];
  color?: string | [number, number, number];
  scale?: number;
  debug?: boolean;
}) {
  const lineRef = useRef<THREE.Line>(null);

  useLayoutEffect(() => {
    if (lineRef.current?.geometry) {
      lineRef.current.geometry.setFromPoints(
        [start, end].map((point) => new THREE.Vector3(...point)),
      );
    } else {
      console.error("(Util.Line) geometry ref is null");
    }
  }, [start, end]);

  return (
    <line_
      ref={lineRef}
      scale={[scale, scale, scale]}
      onPointerOver={() => {
        if (debug) {
          console.log(`start: ${start} end: ${end}`);
        }
      }}
    >
      <bufferGeometry />
      <lineBasicMaterial color={color} />
    </line_>
  );
}

export function GrowLine({
  start,
  end,
  color = "grey",
  scale = 1.0,
  debug = false,
}: {
  start: [number, number, number];
  end: [number, number, number];
  color?: string | [number, number, number];
  scale?: number;
  debug?: boolean;
}) {
  const lineRef = useRef<THREE.Line>(null);
  const MAX_POINTS = 100;

  useLayoutEffect(() => {
    if (lineRef.current?.geometry) {
      const points = [];

      for (let i = 0; i < MAX_POINTS; i++) {
        points.push(
          new THREE.Vector3(
            start[0] + ((end[0] - start[0]) * i) / MAX_POINTS,
            start[1] + ((end[1] - start[1]) * i) / MAX_POINTS,
            start[2] + ((end[2] - start[2]) * i) / MAX_POINTS,
          ),
        );
      }
      lineRef.current.geometry.setFromPoints(points);
    } else {
      console.error("(Util.Line) geometry ref is null");
    }
  }, [start, end]);

  return (
    <line_
      ref={lineRef}
      onPointerOver={() => {
        if (debug) {
          console.log(`start: ${start} end: ${end}`);
        }
      }}
    >
      <bufferGeometry drawRange={{ start: 0, count: MAX_POINTS * scale }} />
      <lineBasicMaterial color={color} />
    </line_>
  );
}

const DEFAULT_RANGE_SPHERE_OPACITY = 0.15;
const DEFAULT_RANGE_SPHERE_COLOR = "#ffffff";

export function RangeSphere({
  pos,
  distance,
  order,
  color = DEFAULT_RANGE_SPHERE_COLOR,
  opacity = DEFAULT_RANGE_SPHERE_OPACITY,
}: {
  pos: [number, number, number];
  distance: number;
  order: number;
  color?: string;
  opacity?: number;
}) {
  return (
    <>
      <mesh position={pos} renderOrder={order}>
        <sphereGeometry args={[distance * SCALE, 40, 40]} />
        {/* BackSide, not DoubleSide: half the surfaces, and you look into the
            shell rather than through both of its faces. `depthWrite: false`
            stops it punching holes in other transparent geometry. */}
        <meshBasicMaterial
          color={color}
          opacity={opacity}
          transparent={true}
          depthWrite={false}
          side={THREE.BackSide}
          wireframe={false}
        />
      </mesh>
    </>
  );
}

/**
 * Enough segments that the circle reads as a circle rather than a polygon.
 * Cheap: this is a line, not a surface.
 */
const RANGE_CIRCLE_SEGMENTS = 96;

/**
 * Label size as a fraction of camera distance, which is what keeps it constant
 * on screen: apparent size goes as world size over distance, so scaling the one
 * by the other cancels out.
 */
const LABEL_SCALE = 0.035;

/** Reused so the per-frame label scaling allocates nothing. */
const LABEL_SCRATCH = new THREE.Vector3();

/**
 * One range band, drawn as the circle you would see looking at a sphere.
 *
 * A sphere centred on a ship has the same silhouette from every viewpoint -- a
 * circle of its radius -- so the circle carries the whole of what a shell has
 * to say. Filled shells were the obvious first try and they do not work: four
 * of them, double-sided, stack eight translucent surfaces over exactly the part
 * of the display you care about, the middle, and transparency sorting turns
 * near-coincident surfaces into a checkerboard.
 *
 * Range is a scalar. Drawing it as a volume is what made it unreadable; drawing
 * it as an annotation costs almost no ink, occludes nothing, and survives
 * several ships being on screen at once.
 *
 * Billboarded, so it stays a true silhouette as the camera moves rather than
 * foreshortening into an ellipse.
 */
export function RangeCircle({
  pos,
  distance,
  label,
  color = DEFAULT_RANGE_SPHERE_COLOR,
  // Every circle at the same weight. Fading outwards made sense for nested
  // shells, where alpha accumulated through every surface in front; lines do
  // not overlap, so a fade only made the outer bands faintest exactly when
  // they are hardest to see -- they are only in frame at all when the camera
  // is far enough out for a hairline to be thin.
  opacity = 0.6,
}: {
  pos: [number, number, number];
  distance: number;
  label?: string;
  color?: string;
  opacity?: number;
}) {
  const groupRef = useRef<THREE.Group>(null);
  const ringRef = useRef<THREE.Group>(null);
  const lineRef = useRef<THREE.Line>(null);
  const labelRef = useRef<THREE.Object3D>(null);
  const { camera } = useThree();
  const radius = distance * SCALE;

  useFrame(() => {
    const group = groupRef.current;
    const ring = ringRef.current;
    if (group == null || ring == null) {
      return;
    }
    group.lookAt(camera.position);
    group.getWorldPosition(LABEL_SCRATCH);
    const cameraDistance = camera.position.distanceTo(LABEL_SCRATCH);

    // Draw the sphere's real outline, not the great circle through its
    // centre. The two differ by enough, when the camera is a few radii out,
    // that a ship inside the sphere was drawn outside the ring -- see
    // `sphereSilhouette`. After `lookAt`, local +z points at the camera.
    const silhouette = sphereSilhouette(radius, cameraDistance);
    ring.visible = silhouette != null;
    if (silhouette == null) {
      return;
    }
    ring.scale.setScalar(silhouette.scale);
    ring.position.z = silhouette.offset;

    // Hold the label at a constant size on screen. Text measured in world
    // units shrinks with distance, and these circles span a 40x range of
    // radii -- the outermost is only in frame from about 65 units out, where
    // a fixed 0.35-unit label is a third of a percent of screen height. The
    // band you can see would be the one you cannot read. Divided by the
    // ring's scale, since the label sits inside the scaled ring.
    const label = labelRef.current;
    if (label != null) {
      label.scale.setScalar((cameraDistance * LABEL_SCALE) / silhouette.scale);
    }
  });

  useLayoutEffect(() => {
    const points = [];
    for (let i = 0; i <= RANGE_CIRCLE_SEGMENTS; i++) {
      const angle = (i / RANGE_CIRCLE_SEGMENTS) * Math.PI * 2;
      points.push(
        new THREE.Vector3(
          Math.cos(angle) * radius,
          Math.sin(angle) * radius,
          0,
        ),
      );
    }
    lineRef.current?.geometry.setFromPoints(points);
  }, [radius]);

  return (
    <group ref={groupRef} position={pos}>
      {/* The ring and its label move together: scaled down and pushed toward
          the camera by exactly the amount that turns the great circle into
          the outline. */}
      <group ref={ringRef}>
      <line_ ref={lineRef}>
        <bufferGeometry />
        {/* `depthWrite: false` on anything transparent -- a transparent surface
            writing depth is what makes chunks of other transparent things
            disappear. */}
        <lineBasicMaterial
          color={color}
          transparent={true}
          opacity={opacity}
          depthWrite={false}
        />
      </line_>
      {label && (
        <Text
          ref={labelRef}
          position={[0, radius, 0]}
          fontSize={1}
          color={color}
          anchorX="center"
          anchorY="bottom"
          fillOpacity={opacity}>
          {label}
        </Text>
      )}
      </group>
    </group>
  );
}

const rangeBandNames = ["Short", "Medium", "Long", "Very Long", "Distant"];
export function findRangeBand(distance: number) {
  let range = RANGE_BANDS.findIndex((x) => x >= distance);
  if (range < 0) {
    range = rangeBandNames.length - 1;
  }
  return rangeBandNames[range];
}
