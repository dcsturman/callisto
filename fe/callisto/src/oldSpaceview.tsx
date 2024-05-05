import * as THREE from "three";
import { TrackballControls } from "three/addons/controls/TrackballControls.js";

import { useEffect, useRef } from "react";

function SpaceView() {
  const refContainer = useRef<HTMLDivElement>(null);
  let controls: TrackballControls, scene: THREE.Scene, camera: THREE.Camera, renderer: THREE.WebGLRenderer;

  function drawAxis() {
    const x_axis = [];
    x_axis.push(new THREE.Vector3(-1000, 0, 0));
    x_axis.push(new THREE.Vector3(1000, 0, 0));

    const y_axis = [];
    y_axis.push(new THREE.Vector3(0, -1000, 0));
    y_axis.push(new THREE.Vector3(0, 1000, 0));

    const z_axis = [];
    z_axis.push(new THREE.Vector3(0, 0, -1000));
    z_axis.push(new THREE.Vector3(0, 0, 1000));

    var x_mat = new THREE.LineBasicMaterial({ color: 0x0000ff });
    var x_geometry = new THREE.BufferGeometry().setFromPoints(x_axis);
    var x_line = new THREE.Line(x_geometry, x_mat);
    scene.add(x_line);

    var y_mat = new THREE.LineBasicMaterial({ color: 0xffff00 });
    var y_geometry = new THREE.BufferGeometry().setFromPoints(y_axis);
    var y_line = new THREE.Line(y_geometry, y_mat);
    scene.add(y_line);

    var z_mat = new THREE.LineBasicMaterial({ color: 0x00ffff });
    var z_geometry = new THREE.BufferGeometry().setFromPoints(z_axis);
    var z_line = new THREE.Line(z_geometry, z_mat);
    scene.add(z_line);
  }

  function init() {
    // === THREE.JS CODE START ===
    scene = new THREE.Scene();
    camera = new THREE.PerspectiveCamera(
      75,
      window.innerWidth / window.innerHeight,
      0.1,
      1000
    );
    renderer = new THREE.WebGLRenderer();
    renderer.setSize(window.innerWidth, window.innerHeight);
    renderer.setPixelRatio( window.devicePixelRatio );

    refContainer.current &&
      refContainer.current.appendChild(renderer.domElement);

    camera.position.z = 10;
    camera.position.y = 10;
    camera.position.x = 10;
    camera.lookAt(0, 0, 0);

    controls = new TrackballControls(camera, renderer.domElement);
    controls.rotateSpeed = 1.0;
    controls.zoomSpeed = 1.2;
    controls.panSpeed = 0.8;

    controls.keys = [ 'KeyA', 'KeyS', 'KeyD' ];
  }

  useEffect(() => {
    // document.body.appendChild( renderer.domElement );
    // use ref as a mount point of the Three.js scene instead of the document.body
    /*var geometry = new THREE.BoxGeometry(1, 1, 1);
    var material = new THREE.MeshBasicMaterial({ color: 0x00ff00 });
    var cube = new THREE.Mesh(geometry, material);
    scene.add(cube);
    var animate = function () {
      requestAnimationFrame(animate);
      cube.rotation.x += 0.01;
      cube.rotation.y += 0.01;

    };
    animate();*/

    init()

    let animate = function() {
        requestAnimationFrame(animate);
        drawAxis();
        controls.update();
        renderer.render(scene, camera);
    }
    animate();
  },[]);
  return <div ref={refContainer}></div>;
}

export default SpaceView;
