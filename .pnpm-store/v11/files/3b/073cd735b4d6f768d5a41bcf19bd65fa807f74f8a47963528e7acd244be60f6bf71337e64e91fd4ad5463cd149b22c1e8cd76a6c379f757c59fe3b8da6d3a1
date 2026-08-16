import type { UseOverlayStateReturn } from "../../hooks/use-overlay-state";
import type { DOMRenderProps } from "../../utils/dom";
import type { DrawerVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { ButtonProps as ButtonPrimitiveProps } from "react-aria-components/Button";
import type { DialogProps as DialogPrimitiveProps } from "react-aria-components/Dialog";
import React from "react";
import { Button as ButtonPrimitive } from "react-aria-components/Button";
import { DialogTrigger as DrawerTriggerPrimitive, Heading as HeadingPrimitive } from "react-aria-components/Dialog";
import { ModalOverlay as ModalOverlayPrimitive, Modal as ModalPrimitive } from "react-aria-components/Modal";
type DrawerPlacement = "top" | "bottom" | "left" | "right";
interface DrawerRootProps extends ComponentPropsWithRef<typeof DrawerTriggerPrimitive> {
    state?: UseOverlayStateReturn;
}
declare const DrawerRoot: {
    ({ children, state, ...props }: DrawerRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerTriggerProps extends ComponentPropsWithRef<typeof ButtonPrimitive> {
}
declare const DrawerTrigger: {
    ({ children, className, ...props }: DrawerTriggerProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerBackdropProps extends ComponentPropsWithRef<typeof ModalOverlayPrimitive> {
    variant?: DrawerVariants["variant"];
    /**
     * Whether to close the drawer when the user interacts outside it.
     * @default true
     */
    isDismissable?: boolean;
}
declare const DrawerBackdrop: {
    ({ children, className, isDismissable, variant, ...props }: DrawerBackdropProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerContentProps extends Omit<ComponentPropsWithRef<typeof ModalPrimitive>, Exclude<keyof DrawerBackdropProps, "children" | "className">> {
    placement?: DrawerPlacement;
}
declare const DrawerContent: {
    ({ children, className, placement, ...props }: DrawerContentProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerDialogProps extends DialogPrimitiveProps {
}
declare const DrawerDialog: {
    ({ children, className, ...props }: DrawerDialogProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerHeaderProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DrawerHeader: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DrawerHeaderProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DrawerHeaderProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerBodyProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DrawerBody: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DrawerBodyProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DrawerBodyProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerFooterProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DrawerFooter: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DrawerFooterProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DrawerFooterProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerHeadingProps extends ComponentPropsWithRef<typeof HeadingPrimitive> {
}
declare const DrawerHeading: {
    ({ children, className, ...props }: DrawerHeadingProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerHandleProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DrawerHandle: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: DrawerHandleProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DrawerHandleProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface DrawerCloseTriggerProps extends ButtonPrimitiveProps {
    className?: string;
    children?: ReactNode;
}
declare const DrawerCloseTrigger: {
    ({ className, ...rest }: DrawerCloseTriggerProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { DrawerRoot, DrawerTrigger, DrawerBackdrop, DrawerContent, DrawerDialog, DrawerHeader, DrawerHeading, DrawerBody, DrawerFooter, DrawerHandle, DrawerCloseTrigger, };
export type { DrawerRootProps, DrawerTriggerProps, DrawerBackdropProps, DrawerContentProps, DrawerDialogProps, DrawerHeaderProps, DrawerHeadingProps, DrawerBodyProps, DrawerFooterProps, DrawerHandleProps, DrawerCloseTriggerProps, };
