import type { DOMRenderProps } from "../../utils/dom";
import type { TooltipVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { OverlayArrow, Tooltip as TooltipPrimitive, TooltipTrigger as TooltipTriggerPrimitive } from "react-aria-components/Tooltip";
type TooltipRootProps = ComponentPropsWithRef<typeof TooltipTriggerPrimitive> & {
    /**
     * Whether to let React Aria skip the enter and exit animations while its global warmup timer is
     * active, i.e. when moving quickly from one tooltip to another.
     *
     * @default false
     */
    shouldSkipAnimation?: boolean;
};
declare const TooltipRoot: ({ children, closeDelay, delay, shouldSkipAnimation, ...props }: TooltipRootProps) => import("react/jsx-runtime").JSX.Element;
interface TooltipContentProps extends Omit<ComponentPropsWithRef<typeof TooltipPrimitive>, "children">, TooltipVariants {
    showArrow?: boolean;
    children: React.ReactNode;
}
declare const TooltipContent: ({ children, className, offset: offsetProp, showArrow, ...props }: TooltipContentProps) => import("react/jsx-runtime").JSX.Element;
type TooltipArrowProps = Omit<ComponentPropsWithRef<typeof OverlayArrow>, "children"> & {
    children?: React.ReactNode;
};
declare const TooltipArrow: ({ children, className, ...props }: TooltipArrowProps) => import("react/jsx-runtime").JSX.Element;
interface TooltipTriggerProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const TooltipTrigger: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: TooltipTriggerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof TooltipTriggerProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { TooltipRoot, TooltipTrigger, TooltipContent, TooltipArrow };
export type { TooltipRootProps, TooltipArrowProps, TooltipContentProps, TooltipTriggerProps };
