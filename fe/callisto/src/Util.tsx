import * as THREE from "three";
import { useLayoutEffect, useRef } from "react";
import { extend } from "@react-three/fiber";

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

export function vectorToString(v: [number, number, number]) {
  return `${v[0].toFixed(0)}, ${v[1].toFixed(0)}, ${v[2].toFixed(0)}`;
}

export function Line({
  start,
  end,
  color = "grey",
  debug = false
}: {
  start: [number, number, number];
  end: [number, number, number];
  color?: string;
  debug?: boolean;
}) {
  const lineRef = useRef<THREE.Line>(null);

  useLayoutEffect(() => {
    if (lineRef.current?.geometry) {
      lineRef.current.geometry.setFromPoints(
        [start, end].map((point) => new THREE.Vector3(...point))
      );
    } else {
      console.log("ref is null");
    }
  }, [start, end]);

  return (
    <line_ ref={lineRef} onPointerOver={()=> { if (debug)  {console.log(`start: ${start} end: ${end}`)}}}>
      <bufferGeometry />
      <lineBasicMaterial color={color} />
    </line_>
  );
}
