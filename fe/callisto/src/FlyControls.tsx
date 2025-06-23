import React from "react";
import {useEffect, useMemo} from "react";
import {Camera, Quaternion, Vector3} from "three";

type FlyControlsProps = {
  containerName: string;
  camera: Camera;
  autoForward: boolean;
  dragToLook: boolean;
  movementSpeed: number;
  rollSpeed: number;
};

export const FlyControls: React.FC<FlyControlsProps> = ({
  containerName,
  camera,
  autoForward,
  dragToLook,
  movementSpeed,
  rollSpeed,
}) => {
  const domElement = useMemo(
    () =>  document.getElementById(containerName) as HTMLElement,
    [containerName]
  );

  const EPS = 0.000001;
  const NO_MOVE = {
    up: 0,
    down: 0,
    left: 0,
    right: 0,
    forward: 0,
    back: 0,
    pitchUp: 0,
    pitchDown: 0,
    yawLeft: 0,
    yawRight: 0,
    rollLeft: 0,
    rollRight: 0,
  };

  useEffect(() => {
    let moveState = {...NO_MOVE};
    let mouseStatus = 0;
    let movementSpeedMultiplier = 1;

    let previousTime = 0;
    let lastPosition = new Vector3();
    let lastQuaternion = new Quaternion();

    const generateMovementVector = (): Vector3 => {
      const forward = moveState.forward || (autoForward && !moveState.back) ? 1 : 0;

      return new Vector3(
        -moveState.left + moveState.right,
        -moveState.down + moveState.up,
        -forward + moveState.back
      );
    };

    const generateRotationalVector = (): Vector3 => {
      return new Vector3(
        -moveState.pitchDown + moveState.pitchUp,
        -moveState.yawRight + moveState.yawLeft,
        -moveState.rollRight + moveState.rollLeft
      );
    };

    const update = (lastTime: DOMHighResTimeStamp) => {
      const delta = (lastTime - previousTime) / 1000;
      previousTime = lastTime;

      const moveMultiplier = delta * movementSpeed * movementSpeedMultiplier;
      const rotationMultiplier = delta * rollSpeed;

      if (!camera) {
        console.error("(FlyControls)Missing Camera in controls.");
        return;
      }

      const moveVector = generateMovementVector();
      const rotationVector = generateRotationalVector();

      camera!.translateX(moveVector.x * moveMultiplier);
      camera!.translateY(moveVector.y * moveMultiplier);
      camera!.translateZ(moveVector.z * moveMultiplier);

      const tmpQuaternion = new Quaternion();
      tmpQuaternion
        .set(rotationVector.x * rotationMultiplier, rotationVector.y * rotationMultiplier, rotationVector.z * rotationMultiplier, 1)
        .normalize();
      camera.quaternion.multiply(tmpQuaternion);

      //? what does this do
      if (
        lastPosition.distanceToSquared(camera.position) > EPS ||
        8 * (1 - lastQuaternion.dot(camera.quaternion)) > EPS
      ) {
        //dispatchEvent(changeEvent);
        lastQuaternion = camera.quaternion;
        lastPosition = camera.position;
      }

      //? Do I need this
      requestAnimationFrame(update);
    };

    const contextmenu = (/*event: MouseEvent*/): void => {
      // Do nothing at this point but its here if we need it.
    };
  
    const keydown = (event: KeyboardEvent): void => {
      if (event.altKey) {
        return;
      }
  
      switch (event.code) {
        case "ShiftLeft":
        case "ShiftRight":
          movementSpeedMultiplier = 100;
          break;
  
        case "KeyW":
          moveState.forward = 1;
          break;
        case "KeyS":
          moveState.back = 1;
          break;
        case "KeyA":
          moveState.left = 1;
          break;
        case "KeyD":
          moveState.right = 1;
          break;
        case "KeyR":
          moveState.up = 1;
          break;
        case "KeyF":
          moveState.down = 1;
          break;
        case "ArrowUp":
          moveState.pitchUp = 1;
          break;
        case "ArrowDown":
          moveState.pitchDown = 1;
          break;
        case "ArrowLeft":
          moveState.yawLeft = 1;
          break;
        case "ArrowRight":
          moveState.yawRight = 1;
          break;
        case "KeyQ":
          moveState.rollLeft = 1;
          break;
        case "KeyE":
          moveState.rollRight = 1;
          break;
      }
    };
  
    const keyup = (event: KeyboardEvent): void => {
      if (["ShiftLeft", "ShiftRight"].includes(event.code)) {
        movementSpeedMultiplier = 1;
      } else {
        moveState = {...NO_MOVE};
      }
    };
  
    const pointerdown = (event: PointerEvent): void => {
      if (dragToLook) {
        mouseStatus = 1;
      } else {
        switch (event.button) {
          case 0:
            moveState.forward = 1;
            break;
          case 2:
            moveState.back = 1;
            break;
        }
      }
    };
  
    const getContainerDimensions = (): {
      size: number[];
      offset: number[];
    } => {
      return {
        size: [domElement.offsetWidth, domElement.offsetHeight],
        offset: [domElement.offsetLeft, domElement.offsetTop],
      };
    };
  
    const pointermove = (event: PointerEvent): void => {      
      if (!dragToLook || mouseStatus > 0) {
        if (event.pressure === 0) {
          mouseStatus = 0;
          // Not sure I want this.
          moveState = {...moveState, yawRight: 0, pitchUp: 0, yawLeft: 0, pitchDown: 0};          
          return;
        }
        const container = getContainerDimensions();
        const halfWidth = container.size[0] / 2;
        const halfHeight = container.size[1] / 2;
  
        moveState = {
          ...moveState,
          yawLeft: -(event.pageX - container.offset[0] - halfWidth) / halfWidth,
          pitchDown: (event.pageY - container.offset[1] - halfHeight) / halfHeight,
        };
      }
    };
  
    const pointerup = (event: PointerEvent): void => {
      if (dragToLook) {
        mouseStatus = 0;
        moveState = {...moveState, yawRight: 0, pitchUp: 0, yawLeft: 0, pitchDown: 0};
      } else {
        switch (event.button) {
          case 0:
            moveState.forward = 0;
            break;
          case 2:
            moveState.back = 0;
            break;
        }
      }
    };
  
    // https://github.com/mrdoob/three.js/issues/20575
    const connect = (domElement: HTMLElement): void => {
      domElement.setAttribute("tabindex", "-1");

      domElement.addEventListener("contextmenu", contextmenu);
      domElement.addEventListener("pointermove", pointermove);
      domElement.addEventListener("pointerdown", pointerdown);
      domElement.addEventListener("pointerup", pointerup);
      window.addEventListener("keydown", keydown);
      window.addEventListener("keyup", keyup);
    };

    if (!camera) {
      return;
    }
    
    connect(domElement);

    const id = window.requestAnimationFrame(update)

    return () => window.cancelAnimationFrame(id); // Cleanup function
  }, [camera]);

  return <></>;
};
