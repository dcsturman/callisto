import { extend, Object3DNode } from '@react-three/fiber'

extend({ Line_: THREE.Line })

declare module '@react-three/fiber' {
  interface ThreeElements {
    line_: Object3DNode<THREE.Line, typeof THREE.Line>
  }
}