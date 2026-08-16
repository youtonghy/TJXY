import * as React from 'react';
import { canAnimate, prefersReducedMotion } from 'number-flow/lite';
import { buildStyles } from 'number-flow/csp';
export * from 'number-flow/plugins';
export { a as NumberFlowElement, b as NumberFlowGroup, N as default } from './NumberFlow-client-BGPmzcXX.mjs';

const styles = buildStyles('-react');
const useIsSupported = ()=>React.useSyncExternalStore(()=>()=>{}, ()=>canAnimate, ()=>false);
const usePrefersReducedMotion = ()=>React.useSyncExternalStore((cb)=>{
        prefersReducedMotion?.addEventListener('change', cb);
        return ()=>prefersReducedMotion?.removeEventListener('change', cb);
    }, ()=>prefersReducedMotion.matches, ()=>false);
function useCanAnimate({ respectMotionPreference = true } = {}) {
    const isSupported = useIsSupported();
    const reducedMotion = usePrefersReducedMotion();
    return isSupported && (!respectMotionPreference || !reducedMotion);
}

export { styles, useCanAnimate, useIsSupported, usePrefersReducedMotion };
