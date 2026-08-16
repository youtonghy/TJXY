import type { DOMRenderProps } from "../../utils/dom";
import type { SkeletonVariants } from "@heroui/styles";
import React from "react";
interface SkeletonRootProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    className?: string;
    /** Animation type. */
    animationType?: SkeletonVariants["animationType"];
}
declare const SkeletonRoot: <E extends keyof React.JSX.IntrinsicElements = "div">({ animationType, className, ...props }: SkeletonRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SkeletonRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { SkeletonRoot };
export type { SkeletonRootProps };
