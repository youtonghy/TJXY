import type { DOMRenderProps } from "../../utils/dom";
import type { SurfaceVariants } from "@heroui/styles";
import type { ReactNode } from "react";
import React from "react";
type SurfaceContext = {
    variant?: SurfaceVariants["variant"];
};
declare const SurfaceContext: React.Context<SurfaceContext>;
interface SurfaceRootProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children: ReactNode;
    className?: string;
    /** Visual variant. @default "default" */
    variant?: SurfaceVariants["variant"];
}
declare const SurfaceRoot: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, variant, ...rest }: SurfaceRootProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SurfaceRootProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { SurfaceRoot, SurfaceContext };
export type { SurfaceRootProps };
