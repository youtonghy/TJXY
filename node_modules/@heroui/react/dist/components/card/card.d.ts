import type { DOMRenderProps } from "../../utils/dom";
import type { CardVariants } from "@heroui/styles";
import type { ReactNode } from "react";
import React from "react";
interface CardRootProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children: ReactNode;
    className?: string;
    /** Visual variant. @default "default" */
    variant?: CardVariants["variant"];
}
declare const CardRoot: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, variant, ...props }: CardRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CardRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface CardHeaderProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const CardHeader: <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: CardHeaderProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CardHeaderProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface CardTitleProps<E extends keyof React.JSX.IntrinsicElements = "h3"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const CardTitle: <E extends keyof React.JSX.IntrinsicElements = "h3">({ children, className, ...props }: CardTitleProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CardTitleProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface CardDescriptionProps<E extends keyof React.JSX.IntrinsicElements = "p"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const CardDescription: <E extends keyof React.JSX.IntrinsicElements = "p">({ children, className, ...props }: CardDescriptionProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CardDescriptionProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface CardContentProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const CardContent: <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: CardContentProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CardContentProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface CardFooterProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const CardFooter: <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: CardFooterProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CardFooterProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { CardRoot, CardHeader, CardTitle, CardDescription, CardContent, CardFooter };
export type { CardRootProps, CardHeaderProps, CardTitleProps, CardDescriptionProps, CardContentProps, CardFooterProps, };
