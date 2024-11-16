import { AuthCodeFlowOptions } from "@react-oauth/google";


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
