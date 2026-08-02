import type { ComponentType, ForwardedRef, ReactElement, ReactNode } from "react";
import React from "react";
/**
 * Create a CollectionBuilder-safe slot for optional chrome around a RAC collection
 * target (e.g. TabList, ListBox).
 *
 * The injector emits no host DOM: chrome props are provided via Context and a
 * string-keyed `cloneElement` channel (React strips Symbol keys from element props).
 * The target should apply chrome through the RAC `render` prop — not by wrapping
 * the collection child in a host node.
 */
export declare const createCollectionSlot: <T extends object>(name: string) => {
    readonly key: string;
    readonly inject: (children: ReactNode, value: T) => ReactNode;
    readonly consume: <P extends object>(props: P) => [injected: T | undefined, rest: P];
    readonly useSlot: <P extends object>(props: P) => [injected: T | undefined, rest: P];
    readonly Injector: {
        ({ children, ...value }: {
            children?: ReactNode;
        } & T): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    readonly withSlot: <P extends object>(Primitive: ComponentType<P & {
        children?: ReactNode;
    }>, defaultRender?: (props: P & {
        children?: ReactNode;
    }, injected: T, forwardedRef: ForwardedRef<unknown>) => ReactElement) => React.ForwardRefExoticComponent<React.PropsWithoutRef<P & {
        children?: ReactNode;
    }> & React.RefAttributes<unknown>>;
    readonly Context: React.Context<T | undefined>;
};
export type CollectionSlot<T extends object> = ReturnType<typeof createCollectionSlot<T>>;
