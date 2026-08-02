import type { UseOverlayStateReturn } from "../../hooks/use-overlay-state";
import type { DOMRenderProps } from "../../utils/dom";
import type { ModalVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { Button as ButtonPrimitive } from "react-aria-components/Button";
import type { DialogProps as DialogPrimitiveProps } from "react-aria-components/Dialog";
import { Heading as HeadingPrimitive, DialogTrigger as ModalTriggerPrimitive } from "react-aria-components/Dialog";
import { ModalOverlay as ModalOverlayPrimitive, Modal as ModalPrimitive } from "react-aria-components/Modal";
type ModalPlacement = "auto" | "top" | "center" | "bottom";
interface ModalRootProps extends ComponentPropsWithRef<typeof ModalTriggerPrimitive> {
    state?: UseOverlayStateReturn;
}
declare const ModalRoot: ({ children, state, ...props }: ModalRootProps) => import("react/jsx-runtime").JSX.Element;
interface ModalTriggerProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ModalTrigger: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ModalTriggerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ModalTriggerProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ModalBackdropProps extends ComponentPropsWithRef<typeof ModalOverlayPrimitive> {
    variant?: ModalVariants["variant"];
    /**
     * Whether to close the modal when the user interacts outside it.
     * @default true
     */
    isDismissable?: boolean;
}
declare const ModalBackdrop: ({ children, className, isDismissable, onClick, variant, ...props }: ModalBackdropProps) => import("react/jsx-runtime").JSX.Element;
interface ModalContainerProps extends Omit<ComponentPropsWithRef<typeof ModalPrimitive>, Exclude<keyof ModalBackdropProps, "children" | "className">> {
    placement?: ModalPlacement;
    scroll?: ModalVariants["scroll"];
    size?: ModalVariants["size"];
}
declare const ModalContainer: ({ children, className, placement, scroll, size, ...props }: ModalContainerProps) => import("react/jsx-runtime").JSX.Element;
interface ModalDialogProps extends DialogPrimitiveProps {
}
declare const ModalDialog: ({ children, className, ...props }: ModalDialogProps) => import("react/jsx-runtime").JSX.Element;
interface ModalHeaderProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ModalHeader: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ModalHeaderProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ModalHeaderProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ModalBodyProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ModalBody: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ModalBodyProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ModalBodyProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ModalFooterProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ModalFooter: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ModalFooterProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ModalFooterProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ModalHeadingProps extends ComponentPropsWithRef<typeof HeadingPrimitive> {
}
declare const ModalHeading: ({ children, className, ...props }: ModalHeadingProps) => import("react/jsx-runtime").JSX.Element;
interface ModalIconProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const ModalIcon: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: ModalIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof ModalIconProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface ModalCloseTriggerProps extends ComponentPropsWithRef<typeof ButtonPrimitive> {
    className?: string;
    children?: ReactNode;
}
declare const ModalCloseTrigger: ({ className, ...rest }: ModalCloseTriggerProps) => import("react/jsx-runtime").JSX.Element;
export { ModalRoot, ModalTrigger, ModalBackdrop, ModalContainer, ModalDialog, ModalHeader, ModalIcon, ModalHeading, ModalBody, ModalFooter, ModalCloseTrigger, };
export type { ModalRootProps, ModalTriggerProps, ModalBackdropProps, ModalContainerProps, ModalDialogProps, ModalHeaderProps, ModalIconProps, ModalHeadingProps, ModalBodyProps, ModalFooterProps, ModalCloseTriggerProps, };
