import type { DOMRenderProps } from "../../utils/dom";
import type { AlertVariants } from "@heroui/styles";
import type { ReactNode } from "react";
import React from "react";
interface AlertRootProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children: ReactNode;
    className?: string;
    /** Alert status. */
    status?: AlertVariants["status"];
}
declare const AlertRoot: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, status, ...rest }: AlertRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AlertRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AlertIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AlertIndicator: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: AlertIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AlertIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AlertContentProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AlertContent: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...rest }: AlertContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AlertContentProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AlertTitleProps<E extends keyof React.JSX.IntrinsicElements = "p"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AlertTitle: <E extends keyof React.JSX.IntrinsicElements = "p">({ children, className, ...rest }: AlertTitleProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AlertTitleProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AlertDescriptionProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AlertDescription: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...rest }: AlertDescriptionProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AlertDescriptionProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { AlertRoot, AlertIndicator, AlertContent, AlertTitle, AlertDescription };
export type { AlertRootProps, AlertIndicatorProps, AlertContentProps, AlertTitleProps, AlertDescriptionProps, };
