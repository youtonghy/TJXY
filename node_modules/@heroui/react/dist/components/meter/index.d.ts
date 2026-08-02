import type { ComponentProps } from "react";
import { MeterFill, MeterOutput, MeterRoot, MeterTrack } from "./meter";
export declare const Meter: {
    ({ children, className, color, size, ...props }: import("./meter").MeterRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Root: {
        ({ children, className, color, size, ...props }: import("./meter").MeterRootProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Output: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./meter").MeterOutputProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./meter").MeterOutputProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Track: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./meter").MeterTrackProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./meter").MeterTrackProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Fill: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ className, style, ...props }: import("./meter").MeterFillProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./meter").MeterFillProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type Meter = {
    Props: ComponentProps<typeof MeterRoot>;
    RootProps: ComponentProps<typeof MeterRoot>;
    OutputProps: ComponentProps<typeof MeterOutput>;
    TrackProps: ComponentProps<typeof MeterTrack>;
    FillProps: ComponentProps<typeof MeterFill>;
};
export { MeterRoot, MeterOutput, MeterTrack, MeterFill };
export type { MeterRootProps, MeterRootProps as MeterProps, MeterOutputProps, MeterTrackProps, MeterFillProps, } from "./meter";
export { meterVariants } from "@heroui/styles";
export type { MeterVariants } from "@heroui/styles";
