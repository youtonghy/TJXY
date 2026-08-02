import type { ComponentProps } from "react";
import { DatePickerPopover, DatePickerRoot, DatePickerTrigger, DatePickerTriggerIndicator } from "./date-picker";
export declare const DatePicker: {
    <T extends import("react-aria/useCalendar").DateValue>({ children, className, onOpenChange, ...props }: import("./date-picker").DatePickerRootProps<T>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Root: {
        <T extends import("react-aria/useCalendar").DateValue>({ children, className, onOpenChange, ...props }: import("./date-picker").DatePickerRootProps<T>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Trigger: {
        ({ children, className, ref, ...props }: import("./date-picker").DatePickerTriggerProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    TriggerIndicator: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./date-picker").DatePickerTriggerIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./date-picker").DatePickerTriggerIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Popover: {
        ({ children, className, placement, ...props }: import("./date-picker").DatePickerPopoverProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type DatePicker = {
    Props: ComponentProps<typeof DatePickerRoot>;
    RootProps: ComponentProps<typeof DatePickerRoot>;
    TriggerProps: ComponentProps<typeof DatePickerTrigger>;
    TriggerIndicatorProps: ComponentProps<typeof DatePickerTriggerIndicator>;
    PopoverProps: ComponentProps<typeof DatePickerPopover>;
};
export { DatePickerRoot, DatePickerTrigger, DatePickerTriggerIndicator, DatePickerPopover };
export type { DatePickerRootProps, DatePickerRootProps as DatePickerProps, DatePickerTriggerProps, DatePickerTriggerIndicatorProps, DatePickerPopoverProps, } from "./date-picker";
export { datePickerVariants } from "@heroui/styles";
export type { DatePickerVariants } from "@heroui/styles";
