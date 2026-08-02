import type { DOMRenderProps } from "../../utils/dom";
import type { SwitchVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { SwitchButtonRenderProps, SwitchFieldRenderProps } from "react-aria-components/Switch";
import React from "react";
import { SwitchButton as SwitchButtonPrimitive, SwitchField as SwitchFieldPrimitive } from "react-aria-components/Switch";
interface SwitchRootProps extends ComponentPropsWithRef<typeof SwitchFieldPrimitive>, SwitchVariants {
}
declare const SwitchRoot: {
    ({ children, className, size, ...props }: SwitchRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface SwitchContentProps extends ComponentPropsWithRef<typeof SwitchButtonPrimitive> {
}
declare const SwitchContent: {
    ({ children, className, ...props }: SwitchContentProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface SwitchControlProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const SwitchControl: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: SwitchControlProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SwitchControlProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface SwitchThumbProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const SwitchThumb: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: SwitchThumbProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SwitchThumbProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface SwitchIconProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const SwitchIcon: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: SwitchIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SwitchIconProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { SwitchRoot, SwitchContent, SwitchControl, SwitchThumb, SwitchIcon };
export type { SwitchRootProps, SwitchContentProps, SwitchControlProps, SwitchThumbProps, SwitchIconProps, SwitchFieldRenderProps, SwitchButtonRenderProps, };
