import type { DOMRenderProps } from "../../utils/dom";
import type { MeterVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { Meter as MeterPrimitive } from "react-aria-components/Meter";
interface MeterRootProps extends ComponentPropsWithRef<typeof MeterPrimitive>, MeterVariants {
}
declare const MeterRoot: {
    ({ children, className, color, size, ...props }: MeterRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface MeterOutputProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const MeterOutput: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: MeterOutputProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof MeterOutputProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface MeterTrackProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const MeterTrack: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: MeterTrackProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof MeterTrackProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface MeterFillProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
    style?: React.CSSProperties;
}
declare const MeterFill: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ className, style, ...props }: MeterFillProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof MeterFillProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { MeterRoot, MeterOutput, MeterTrack, MeterFill };
export type { MeterRootProps, MeterOutputProps, MeterTrackProps, MeterFillProps };
