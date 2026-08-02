import type { DOMRenderProps } from "../../utils/dom";
import type { DisclosureVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { ButtonProps } from "react-aria-components/Button";
import React from "react";
import { DisclosurePanel, Disclosure as DisclosurePrimitive } from "react-aria-components/Disclosure";
import { Heading as DisclosureHeadingPrimitive } from "react-aria-components/Heading";
interface DisclosureRootProps extends ComponentPropsWithRef<typeof DisclosurePrimitive>, DisclosureVariants {
}
declare const DisclosureRoot: ({ children, className, ...props }: DisclosureRootProps) => import("react/jsx-runtime").JSX.Element;
interface DisclosureHeadingProps extends ComponentPropsWithRef<typeof DisclosureHeadingPrimitive> {
    className?: string;
}
declare const DisclosureHeading: ({ className, ...props }: DisclosureHeadingProps) => import("react/jsx-runtime").JSX.Element;
interface DisclosureTriggerProps extends ButtonProps {
}
declare const DisclosureTrigger: ({ className, ...props }: DisclosureTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface DisclosureContentProps extends ComponentPropsWithRef<typeof DisclosurePanel> {
}
declare const DisclosureContent: ({ children, className, ...props }: DisclosureContentProps) => import("react/jsx-runtime").JSX.Element;
interface DisclosureBodyContentProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DisclosureBody: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DisclosureBodyContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DisclosureBodyContentProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface DisclosureIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "svg"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DisclosureIndicator: <E extends keyof React.JSX.IntrinsicElements = "svg">({ children, className, ...props }: DisclosureIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DisclosureIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { DisclosureRoot, DisclosureHeading, DisclosureTrigger, DisclosureContent, DisclosureBody, DisclosureIndicator, };
export type { DisclosureRootProps, DisclosureContentProps, DisclosureHeadingProps, DisclosureTriggerProps, DisclosureIndicatorProps, DisclosureBodyContentProps, };
