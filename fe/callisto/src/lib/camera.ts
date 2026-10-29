import * as THREE from "three";
import { SCALE } from "lib/universal";
import { store } from "state/store";
import { setCameraPos } from "state/uiSlice";

/**
 * Put the camera `back` scene units behind `position` (metres), looking the
 * way it already looks, so the point lands in the middle of the view.
 */
export function moveCameraTo(
  cameraQuaternion: [number, number, number, number],
  position: [number, number, number],
  back = 40,
) {
  const behind = new THREE.Vector3(0, 0, back).applyQuaternion(
    new THREE.Quaternion(cameraQuaternion[0], cameraQuaternion[1], cameraQuaternion[2], cameraQuaternion[3]),
  );
  const at = new THREE.Vector3(position[0] * SCALE, position[1] * SCALE, position[2] * SCALE).add(behind);
  store.dispatch(setCameraPos({ x: at.x, y: at.y, z: at.z }));
}
