import type { DOMRenderProps } from "../../utils/dom";
import type { PopoverVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { Dialog as DialogPrimitive, Heading as HeadingPrimitive, DialogTrigger as PopoverTriggerPrimitive } from "react-aria-components/Dialog";
import { OverlayArrow, Popover as PopoverPrimitive } from "react-aria-components/Popover";
type PopoverRootProps = ComponentPropsWithRef<typeof PopoverTriggerPrimitive>;
declare const PopoverRoot: ({ children, ...props }: ComponentPropsWithRef<typeof PopoverTriggerPrimitive>) => import("react/jsx-runtime").JSX.Element;
interface PopoverContentProps extends Omit<ComponentPropsWithRef<typeof PopoverPrimitive>, "children">, PopoverVariants {
    children: React.ReactNode;
}
declare const PopoverContent: ({ children, className, ...props }: PopoverContentProps) => import("react/jsx-runtime").JSX.Element;
type PopoverArrowProps = Omit<ComponentPropsWithRef<typeof OverlayArrow>, "children"> & {
    children?: React.ReactNode;
};
declare const PopoverArrow: ({ children, className, ...props }: PopoverArrowProps) => import("react/jsx-runtime").JSX.Element;
type PopoverDialogProps = Omit<ComponentPropsWithRef<typeof DialogPrimitive>, "children"> & {
    children: React.ReactNode;
};
declare const PopoverDialog: ({ children, className, ...props }: PopoverDialogProps) => import("react/jsx-runtime").JSX.Element;
interface PopoverTriggerProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const PopoverTrigger: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: PopoverTriggerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof PopoverTriggerProps<E>>) => import("react/jsx-runtime").JSX.Element;
type PopoverHeadingProps = ComponentPropsWithRef<typeof HeadingPrimitive> & {};
declare const PopoverHeading: ({ children, className, ...props }: PopoverHeadingProps) => import("react/jsx-runtime").JSX.Element;
export { PopoverRoot, PopoverTrigger, PopoverDialog, PopoverArrow, PopoverContent, PopoverHeading };
export type { PopoverRootProps, PopoverTriggerProps, PopoverDialogProps, PopoverArrowProps, PopoverContentProps, PopoverHeadingProps, };
