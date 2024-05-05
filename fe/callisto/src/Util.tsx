
import * as THREE from "three";
import { useEffect, useLayoutEffect, useRef, RefObject } from "react";
import { extend, useLoader } from "@react-three/fiber";

extend({ Line_: THREE.Line });

export function Line({
    start,
    end,
    color="grey"
  }: {
    start: [number, number, number];
    end: [number, number, number];
    color?: string;
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
      <line_ ref={lineRef}>
        <bufferGeometry />
        <lineBasicMaterial color={color} />
      </line_>
    );
  }