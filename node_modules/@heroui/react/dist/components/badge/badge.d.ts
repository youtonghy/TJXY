import type { DOMRenderProps } from "../../utils/dom";
import type { BadgeVariants } from "@heroui/styles";
import type { ReactNode } from "react";
import React from "react";
interface BadgeAnchorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    className?: string;
    children: ReactNode;
}
declare const BadgeAnchor: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: BadgeAnchorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof BadgeAnchorProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface BadgeRootProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
    /** Badge color. */
    color?: BadgeVariants["color"];
    /** Badge placement. */
    placement?: BadgeVariants["placement"];
    /** Badge size. */
    size?: BadgeVariants["size"];
    /** Badge variant. */
    variant?: BadgeVariants["variant"];
}
declare const BadgeRoot: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, color, placement, size, variant, ...props }: BadgeRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof BadgeRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface BadgeLabelProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const BadgeLabel: <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: BadgeLabelProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof BadgeLabelProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { BadgeRoot, BadgeLabel, BadgeAnchor };
export type { BadgeRootProps, BadgeLabelProps, BadgeAnchorProps };
