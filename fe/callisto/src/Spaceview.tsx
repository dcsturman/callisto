import * as THREE from "three";
import { TrackballControls } from "three/addons/controls/TrackballControls.js";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { BlurPass, Resizer, KernelSize, Resolution } from "postprocessing";
import { TextureLoader } from "three/src/loaders/TextureLoader";
import { useLoader } from "@react-three/fiber";

import { Line } from "./Util";

import { scale } from "./Contexts";

function Sun() {
  const origColor = 0xfdb813;
  const colorR = 0.988;
  const colorG = 0.719;
  const colorB = 0.0742;
  const intensity_factor = 2.5;
  // Hacked this to make it smaller for now.
  const sunRadiusMeters = 1.3927e9 / 1000.0;
  const sunRadiusUnits = sunRadiusMeters * scale;

  { console.log("Sun radius: " + sunRadiusUnits) }

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
      <mesh position={[0, 0, 0]}>
        <icosahedronGeometry args={[sunRadiusUnits, 15]} />
        {/* 0xFDB813 */}
        <meshBasicMaterial
          color={[
            colorR * intensity_factor,
            colorG * intensity_factor,
            colorB * intensity_factor,
          ]}
        />
      </mesh>
    </>
  );
}

function Galaxy() {
  const starColorMap = useLoader(TextureLoader, "galaxy1.png");

  return (
    <>
      <mesh>
        <sphereGeometry args={[5000, 64, 64]} />
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
      <Line start={[-1000, 0, 0]} end={[1000, 0, 0]} color="blue"/>
      <Line start={[0, -1000, 0]} end={[0, 1000, 0]} color="green"/>
      <Line start={[0, 0, -1000]} end={[0, 0, 1000]} />
    </>
  );
}

function SpaceView() {
  return (
    <>
      <Axes />
      <Sun />
      <Galaxy />
    </>
  );
}

export default SpaceView;
