/// <reference types="react-scripts" />

import { AuthCodeFlowOptions } from "@react-oauth/google";
import { extend, Object3DNode } from '@react-three/fiber'
import { ReactThreeFiber } from '@react-three/fiber';
import * as THREE from 'three';
import { FlyControls as FlyControlsImpl } from './lib/FlyControls';
import { ForwardRefComponent } from '@react-three/drei/helpers/ts-utils';

declare type UseGoogleLoginOptionsAuthCodeFlow = {
    flow?: 'auth-code';
    accessType?: 'offline' | 'online';
    isSignedIn: boolean;
    responseType?: 'code' | 'token';
    prompt?: '' | 'none' | 'consent' | 'select_account';
    ux_mode?: 'popup' | 'redirect';
} & AuthCodeFlowOptions;

declare module "@react-oauth/google" {
    export function useGoogleLogin(options: UseGoogleLoginOptionsAuthCodeFlow): () => void;
}

extend({ Line_: THREE.Line })

declare module '@react-three/fiber' {
  interface ThreeElements {
    line_: Object3DNode<THREE.Line, typeof THREE.Line>
  }
}

export type FlyControlsProps = ReactThreeFiber.Object3DNode<FlyControlsImpl, typeof FlyControlsImpl> & {
    onChange?: (e?: THREE.Event) => void;
    domElement?: HTMLElement;
    makeDefault?: boolean;
};
export declare const FlyControls: ForwardRefComponent<FlyControlsProps, FlyControlsImpl>;

