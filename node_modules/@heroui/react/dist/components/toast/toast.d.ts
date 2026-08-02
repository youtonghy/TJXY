import type { ToastContentValue } from "./toast-queue";
import type { DOMRenderProps } from "../../utils/dom";
import type { ToastVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { ToastProps as ToastPrimitiveProps } from "react-aria-components/Toast";
import React from "react";
import { Text as TextPrimitive } from "react-aria-components/Text";
import { UNSTABLE_ToastContent as ToastContentPrimitive, UNSTABLE_ToastRegion as ToastRegionPrimitive } from "react-aria-components/Toast";
import { Button } from "../button";
import { CloseButton } from "../close-button";
import { ToastQueue } from "./toast-queue";
interface ToastProps<T extends object = ToastContentValue> extends ToastPrimitiveProps<T>, ToastVariants {
    scaleFactor?: number;
}
declare const Toast: {
    <T extends object = ToastContentValue>({ children, className, placement, scaleFactor, toast, variant, ...rest }: ToastProps<T>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface ToastContentProps extends ComponentPropsWithRef<typeof ToastContentPrimitive> {
}
declare const ToastContent: ({ children, className, ...rest }: ToastContentProps) => import("react/jsx-runtime").JSX.Element;
interface ToastIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
    variant?: ToastVariants["variant"];
}
declare const ToastIndicator: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, variant, ...rest }: ToastIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ToastIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface ToastTitleProps extends ComponentPropsWithRef<typeof TextPrimitive> {
}
declare const ToastTitle: {
    ({ children, className, ...rest }: ToastTitleProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface ToastDescriptionProps extends ComponentPropsWithRef<typeof TextPrimitive> {
}
declare const ToastDescription: {
    ({ children, className, ...rest }: ToastDescriptionProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface ToastCloseButtonProps extends ComponentPropsWithRef<typeof CloseButton> {
}
declare const ToastCloseButton: {
    ({ className, ...rest }: ToastCloseButtonProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface ToastActionButtonProps extends ComponentPropsWithRef<typeof Button> {
}
declare const ToastActionButton: {
    ({ children, className, ...rest }: ToastActionButtonProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
type ToastRegionPrimitiveProps<T extends object = ToastContentValue> = ComponentPropsWithRef<typeof ToastRegionPrimitive<T>>;
interface ToastProviderProps<T extends object = ToastContentValue> extends Omit<ToastRegionPrimitiveProps<T>, "queue" | "children"> {
    children?: ToastRegionPrimitiveProps<T>["children"];
    /** The gap between toasts. @default 8 */
    gap?: number;
    /** The maximum number of toasts to display at a time (visual only). */
    maxVisibleToasts?: number;
    /** The scale factor for toasts. @default 0.05 */
    scaleFactor?: number;
    placement?: ToastVariants["placement"];
    queue?: ToastQueue<T>;
    /** The width of the toast. @default 460 */
    width?: number | string;
}
declare const ToastProvider: {
    <T extends object = ToastContentValue>({ children, className, gap, maxVisibleToasts, placement, queue: queueProp, scaleFactor, width, ...rest }: ToastProviderProps<T>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { ToastQueue, Toast, ToastActionButton, ToastCloseButton, ToastContent, ToastDescription, ToastIndicator, ToastProvider, ToastTitle, };
export type { ToastActionButtonProps, ToastCloseButtonProps, ToastContentProps, ToastDescriptionProps, ToastIndicatorProps, ToastProps, ToastProviderProps, ToastTitleProps, };
