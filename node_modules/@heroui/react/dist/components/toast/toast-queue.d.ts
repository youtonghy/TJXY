import type { ButtonProps } from "../button";
import type { ReactNode } from "react";
import type { ToastOptions as RACToastOptions, UNSTABLE_ToastQueue as ToastQueuePrimitiveType } from "react-aria-components/Toast";
export interface ToastQueueOptions {
    /** The maximum number of toasts to display at a time (visual only). */
    maxVisibleToasts?: number;
    /** Function to wrap updates in (i.e. document.startViewTransition()). */
    wrapUpdate?: (fn: () => void) => void;
}
/** The underlying react-stately queue passed to `ToastRegion` (not the HeroUI `ToastQueue` wrapper). */
export type StatelyToastQueue<T extends object = ToastContentValue> = ToastQueuePrimitiveType<T>;
export declare class ToastQueue<T extends object = ToastContentValue> {
    private queue;
    readonly maxVisibleToasts?: number;
    constructor(options?: ToastQueueOptions);
    add(content: T, options?: RACToastOptions): string;
    close(key: string): void;
    pauseAll(): void;
    resumeAll(): void;
    clear(): void;
    subscribe(fn: () => void): () => void;
    get visibleToasts(): import("react-aria-components").QueuedToast<T>[];
    getQueue(): ToastQueuePrimitiveType<T>;
}
export interface ToastContentValue {
    indicator?: ReactNode | undefined;
    title?: ReactNode | undefined;
    description?: ReactNode | undefined;
    variant?: "default" | "accent" | "success" | "warning" | "danger" | undefined;
    actionProps?: ButtonProps | undefined;
    isLoading?: boolean | undefined;
}
export interface HeroUIToastOptions {
    description?: ReactNode;
    indicator?: ReactNode;
    variant?: ToastContentValue["variant"];
    actionProps?: ButtonProps;
    isLoading?: boolean;
    timeout?: number;
    onClose?: () => void;
}
export interface ToastPromiseOptions<T = unknown> {
    loading: ReactNode;
    success: ((data: T) => ReactNode) | ReactNode;
    error: ((error: Error) => ReactNode) | ReactNode;
}
declare const toastQueue: ToastQueue<ToastContentValue>;
export declare const toast: {
    (message: ReactNode, options?: HeroUIToastOptions): string;
    success(message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">): string;
    danger(message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">): string;
    info(message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">): string;
    warning(message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">): string;
    promise<T>(promise: Promise<T> | (() => Promise<T>), options: ToastPromiseOptions<T>): string;
    getQueue(): ToastQueuePrimitiveType<ToastContentValue>;
    close(key: string): void;
    pauseAll(): void;
    resumeAll(): void;
    clear(): void;
} & {
    success: (message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">) => string;
    danger: (message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">) => string;
    info: (message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">) => string;
    warning: (message: ReactNode, options?: Omit<HeroUIToastOptions, "variant">) => string;
    promise: <T>(promise: Promise<T> | (() => Promise<T>), options: ToastPromiseOptions<T>) => string;
    getQueue: () => ReturnType<() => ToastQueuePrimitiveType<ToastContentValue>>;
    close: (key: string) => void;
    pauseAll: () => void;
    resumeAll: () => void;
    clear: () => void;
};
export { toastQueue };
