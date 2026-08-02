import type { ComponentProps } from "react";
import { DrawerBackdrop, DrawerBody, DrawerCloseTrigger, DrawerContent, DrawerDialog, DrawerFooter, DrawerHandle, DrawerHeader, DrawerHeading, DrawerRoot, DrawerTrigger } from "./drawer";
export declare const Drawer: {
    ({ children, state, ...props }: import("./drawer").DrawerRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Root: {
        ({ children, state, ...props }: import("./drawer").DrawerRootProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Trigger: {
        ({ children, className, ...props }: import("./drawer").DrawerTriggerProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Backdrop: {
        ({ children, className, isDismissable, variant, ...props }: import("./drawer").DrawerBackdropProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Content: {
        ({ children, className, placement, ...props }: import("./drawer").DrawerContentProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Dialog: {
        ({ children, className, ...props }: import("./drawer").DrawerDialogProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Header: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./drawer").DrawerHeaderProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./drawer").DrawerHeaderProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Heading: {
        ({ children, className, ...props }: import("./drawer").DrawerHeadingProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Body: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./drawer").DrawerBodyProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./drawer").DrawerBodyProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Footer: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: import("./drawer").DrawerFooterProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./drawer").DrawerFooterProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Handle: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ className, ...props }: import("./drawer").DrawerHandleProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./drawer").DrawerHandleProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    CloseTrigger: {
        ({ className, ...rest }: import("./drawer").DrawerCloseTriggerProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type Drawer = {
    Props: ComponentProps<typeof DrawerRoot>;
    RootProps: ComponentProps<typeof DrawerRoot>;
    TriggerProps: ComponentProps<typeof DrawerTrigger>;
    BackdropProps: ComponentProps<typeof DrawerBackdrop>;
    ContentProps: ComponentProps<typeof DrawerContent>;
    DialogProps: ComponentProps<typeof DrawerDialog>;
    HeaderProps: ComponentProps<typeof DrawerHeader>;
    HeadingProps: ComponentProps<typeof DrawerHeading>;
    BodyProps: ComponentProps<typeof DrawerBody>;
    FooterProps: ComponentProps<typeof DrawerFooter>;
    HandleProps: ComponentProps<typeof DrawerHandle>;
    CloseTriggerProps: ComponentProps<typeof DrawerCloseTrigger>;
};
export { DrawerRoot, DrawerTrigger, DrawerBackdrop, DrawerContent, DrawerDialog, DrawerHeader, DrawerHeading, DrawerBody, DrawerFooter, DrawerHandle, DrawerCloseTrigger, };
export type { DrawerRootProps, DrawerRootProps as DrawerProps, DrawerTriggerProps, DrawerBackdropProps, DrawerContentProps, DrawerDialogProps, DrawerHeaderProps, DrawerHeadingProps, DrawerBodyProps, DrawerFooterProps, DrawerHandleProps, DrawerCloseTriggerProps, } from "./drawer";
export { drawerVariants } from "@heroui/styles";
export type { DrawerVariants } from "@heroui/styles";
