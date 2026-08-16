import type { DOMRenderProps } from "../../utils/dom";
import type { AlertDialogVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, HTMLAttributes, ReactNode } from "react";
import type { ButtonProps as ButtonPrimitiveProps } from "react-aria-components/Button";
import type { DialogProps as DialogPrimitiveProps } from "react-aria-components/Dialog";
import { DialogTrigger as AlertDialogTriggerPrimitive, Heading as HeadingPrimitive } from "react-aria-components/Dialog";
import { ModalOverlay as ModalOverlayPrimitive, Modal as ModalPrimitive } from "react-aria-components/Modal";
type AlertDialogPlacement = "auto" | "top" | "center" | "bottom";
type AlertDialogStatus = "default" | "accent" | "success" | "warning" | "danger";
interface AlertDialogRootProps extends ComponentPropsWithRef<typeof AlertDialogTriggerPrimitive> {
}
declare const AlertDialogRoot: ({ children, ...props }: AlertDialogRootProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogTriggerProps extends HTMLAttributes<HTMLDivElement> {
}
declare const AlertDialogTrigger: ({ children, className, ...props }: AlertDialogTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogBackdropProps extends ComponentPropsWithRef<typeof ModalOverlayPrimitive> {
    variant?: AlertDialogVariants["variant"];
    /**
     * Whether to close the alert dialog when the user interacts outside it.
     * Alert dialogs typically require explicit action, so this defaults to false.
     * @default false
     */
    isDismissable?: boolean;
    /**
     * Whether pressing the escape key to close the modal should be disabled.
     * Alert dialogs typically require explicit action, so this defaults to true.
     * @default true
     */
    isKeyboardDismissDisabled?: boolean;
}
declare const AlertDialogBackdrop: ({ children, className, isDismissable, isKeyboardDismissDisabled, onClick, variant, ...props }: AlertDialogBackdropProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogContainerProps extends Omit<ComponentPropsWithRef<typeof ModalPrimitive>, Exclude<keyof AlertDialogBackdropProps, "children" | "className">> {
    /**
     * The placement of the alert dialog on the screen.
     * @default "auto"
     */
    placement?: AlertDialogPlacement;
    size?: AlertDialogVariants["size"];
}
declare const AlertDialogContainer: ({ children, className, placement, size, ...props }: AlertDialogContainerProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogDialogProps extends DialogPrimitiveProps {
}
declare const AlertDialogDialog: ({ children, className, ...props }: AlertDialogDialogProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogHeaderProps extends HTMLAttributes<HTMLDivElement> {
}
declare const AlertDialogHeader: ({ children, className, ...props }: AlertDialogHeaderProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogHeadingProps extends ComponentPropsWithRef<typeof HeadingPrimitive> {
}
declare const AlertDialogHeading: ({ children, className, ...props }: AlertDialogHeadingProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogBodyProps extends HTMLAttributes<HTMLDivElement> {
}
declare const AlertDialogBody: ({ children, className, ...props }: AlertDialogBodyProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogFooterProps extends HTMLAttributes<HTMLDivElement> {
}
declare const AlertDialogFooter: ({ children, className, ...props }: AlertDialogFooterProps) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogIconProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
    /**
     * The semantic status of the icon, affects background color and default icon
     * @default "danger"
     */
    status?: AlertDialogStatus;
}
declare const AlertDialogIcon: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, status, ...props }: AlertDialogIconProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AlertDialogIconProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AlertDialogCloseTriggerProps extends ButtonPrimitiveProps {
}
declare const AlertDialogCloseTrigger: ({ className, ...rest }: AlertDialogCloseTriggerProps) => import("react/jsx-runtime").JSX.Element;
export { AlertDialogRoot, AlertDialogTrigger, AlertDialogBackdrop, AlertDialogContainer, AlertDialogDialog, AlertDialogHeader, AlertDialogHeading, AlertDialogBody, AlertDialogFooter, AlertDialogIcon, AlertDialogCloseTrigger, };
export type { AlertDialogRootProps, AlertDialogTriggerProps, AlertDialogBackdropProps, AlertDialogContainerProps, AlertDialogDialogProps, AlertDialogHeaderProps, AlertDialogHeadingProps, AlertDialogBodyProps, AlertDialogFooterProps, AlertDialogIconProps, AlertDialogCloseTriggerProps, AlertDialogStatus, };
