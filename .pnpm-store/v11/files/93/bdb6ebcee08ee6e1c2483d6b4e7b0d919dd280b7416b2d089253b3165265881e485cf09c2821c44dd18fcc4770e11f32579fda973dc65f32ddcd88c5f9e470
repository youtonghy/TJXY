import type { ComponentProps } from "react";
import { ToastActionButton, ToastCloseButton, ToastContent, ToastDescription, ToastIndicator, ToastProvider, Toast as ToastRoot, ToastTitle } from "./toast";
import { ToastQueue, toast, toastQueue } from "./toast-queue";
export declare const Toast: {
    <T extends object = import("./toast-queue").ToastContentValue>({ children, className, placement, scaleFactor, toast, variant, ...rest }: import("./toast").ToastProps<T>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Provider: {
        <T extends object = import("./toast-queue").ToastContentValue>({ children, className, gap, maxVisibleToasts, placement, queue: queueProp, scaleFactor, width, ...rest }: import("./toast").ToastProviderProps<T>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Content: ({ children, className, ...rest }: import("./toast").ToastContentProps) => import("react/jsx-runtime").JSX.Element;
    Indicator: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, variant, ...rest }: import("./toast").ToastIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./toast").ToastIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Title: {
        ({ children, className, ...rest }: import("./toast").ToastTitleProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Description: {
        ({ children, className, ...rest }: import("./toast").ToastDescriptionProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    ActionButton: {
        ({ children, className, ...rest }: import("./toast").ToastActionButtonProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    CloseButton: {
        ({ className, ...rest }: import("./toast").ToastCloseButtonProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Queue: typeof ToastQueue;
    toast: {
        (message: import("react").ReactNode, options?: import("./toast-queue").HeroUIToastOptions): string;
        success(message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">): string;
        danger(message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">): string;
        info(message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">): string;
        warning(message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">): string;
        promise<T>(promise: Promise<T> | (() => Promise<T>), options: import("./toast-queue").ToastPromiseOptions<T>): string;
        getQueue(): import("react-aria-components").UNSTABLE_ToastQueue<import("./toast-queue").ToastContentValue>;
        close(key: string): void;
        pauseAll(): void;
        resumeAll(): void;
        clear(): void;
    } & {
        success: (message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">) => string;
        danger: (message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">) => string;
        info: (message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">) => string;
        warning: (message: import("react").ReactNode, options?: Omit<import("./toast-queue").HeroUIToastOptions, "variant">) => string;
        promise: <T>(promise: Promise<T> | (() => Promise<T>), options: import("./toast-queue").ToastPromiseOptions<T>) => string;
        getQueue: () => ReturnType<() => import("react-aria-components").UNSTABLE_ToastQueue<import("./toast-queue").ToastContentValue>>;
        close: (key: string) => void;
        pauseAll: () => void;
        resumeAll: () => void;
        clear: () => void;
    };
};
export type Toast = {
    Props: ComponentProps<typeof ToastRoot>;
    ProviderProps: ComponentProps<typeof ToastProvider>;
    ContentProps: ComponentProps<typeof ToastContent>;
    IndicatorProps: ComponentProps<typeof ToastIndicator>;
    TitleProps: ComponentProps<typeof ToastTitle>;
    DescriptionProps: ComponentProps<typeof ToastDescription>;
    ActionProps: ComponentProps<typeof ToastActionButton>;
    CloseButtonProps: ComponentProps<typeof ToastCloseButton>;
};
export { ToastProvider, ToastContent, ToastIndicator, ToastTitle, ToastDescription, ToastActionButton, ToastCloseButton, };
export type { ToastCloseButtonProps, ToastProviderProps, ToastContentProps, ToastDescriptionProps, ToastIndicatorProps, ToastProps, ToastTitleProps, } from "./toast";
export { toastVariants } from "@heroui/styles";
export type { ToastVariants } from "@heroui/styles";
export { ToastQueue, toast, toastQueue };
export { DEFAULT_MAX_VISIBLE_TOAST, DEFAULT_GAP, DEFAULT_TOAST_TIMEOUT } from "./constants";
export type { ToastQueueOptions, ToastContentValue } from "./toast-queue";
