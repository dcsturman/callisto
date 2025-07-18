import * as React from "react";
import * as THREE from "three";
import { useLayoutEffect, useRef } from "react";
import { extend } from "@react-three/fiber";
import { SCALE, RANGE_BANDS } from "../Universal";

extend({ Line_: THREE.Line });

/* Entities come in from the server with all units in meters (m). Convert them to the units we can use on screen. */
export function scaleVector(
  v: [number, number, number],
  scale: number
): [number, number, number] {
  return v.map((x) => x * scale) as [number, number, number];
}

export function addVector(a: [number, number, number], b: [number, number, number]) {
  return a.map((x, i) => x + b[i]) as [number, number, number];
}

export function vectorToString(v: [number, number, number], precision: number = 0) {
  return `${v[0].toFixed(precision)}, ${v[1].toFixed(precision)}, ${v[2].toFixed(precision)}`;
}

export function vectorDistance(a: [number, number, number], b: [number, number, number]) {
  return Math.sqrt(
    (a[0] - b[0]) * (a[0] - b[0]) +
      (a[1] - b[1]) * (a[1] - b[1]) +
      (a[2] - b[2]) * (a[2] - b[2])
  );
}

export function Line({
  start,
  end,
  color = "grey",
  scale = 1.0,
  debug = false
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
        [start, end].map((point) => new THREE.Vector3(...point))
      );
    } else {
      console.error("(Util.Line) geometry ref is null");
    }
  }, [start, end]);

  return (
    <line_ ref={lineRef} scale={[scale, scale, scale]} onPointerOver={()=> { if (debug)  {console.log(`start: ${start} end: ${end}`)}}}>
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
  debug = false
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
            start[0] + (end[0] - start[0]) * i / MAX_POINTS,
            start[1] + (end[1] - start[1]) * i / MAX_POINTS,
            start[2] + (end[2] - start[2]) * i / MAX_POINTS
          )
        );
      }
      lineRef.current.geometry.setFromPoints(points);
    } else {
      console.error("(Util.Line) geometry ref is null");
    }
  }, [start, end]);

  return (
    <line_ ref={lineRef} onPointerOver={()=> { if (debug)  {console.log(`start: ${start} end: ${end}`)}}}>
      <bufferGeometry drawRange={{start: 0, count: MAX_POINTS*scale}}/>
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
  opacity = DEFAULT_RANGE_SPHERE_OPACITY
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
        <sphereGeometry args={[distance * SCALE, 15, 15]} />
        <meshLambertMaterial
          color={color}
          opacity={opacity}
          alphaToCoverage={true}
          shadowSide={THREE.FrontSide}
          transparent={true}
          side={THREE.DoubleSide}
        />
      </mesh>
    </>
  );
}

const rangeBandNames = ["Short", "Medium", "Long", "Very Long", "Distant"];
export function findRangeBand(distance: number) {
  let range =RANGE_BANDS.findIndex((x) => x >= distance);
  if (range < 0) {
    range = rangeBandNames.length - 1;
  }
  return rangeBandNames[range];
}
