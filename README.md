# callisto

Callisto is a vector-based space flight and combat simulator based.  By vector-based this means a ship's motion is based on the physics of acceleration and velocity, with acceleration being the only tool for piloting a craft.  There is no banking or rapid breaking in a vector-based system!
Callisto scenarios can be populated with ships and planets. Ships can be outfitted with various weapons and sensors.  Each ship enters orders and they are all resolved simultaneously.  A ship's computer allows simple humans to effectively pilot based on acceleration vectors in 3D space.  The heart of the computer is a nonlinear equation solver.

The code is structure in two main parts.  The `callisto` directory contains a Rust crate containing the server and all of the game logic.  It supports a well defined JSON API for communicating with the client.  The `fe/callisto` directory contains the frontend.  The frontend is a React app that uses Three.js for rendering.