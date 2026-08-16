import type { DOMRenderProps } from "../../utils/dom";
import type { AutocompleteVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import React from "react";
import { Autocomplete as AutocompletePrimitive } from "react-aria-components/Autocomplete";
import { Group as GroupPrimitive } from "react-aria-components/Group";
import { Popover as PopoverPrimitive } from "react-aria-components/Popover";
import { Select as SelectPrimitive, SelectValue as SelectValuePrimitive } from "react-aria-components/Select";
interface AutocompleteRootProps<T extends object, M extends "single" | "multiple" = "single"> extends ComponentPropsWithRef<typeof SelectPrimitive<T, M>>, AutocompleteVariants {
    items?: Iterable<T, M>;
    onClear?: () => void;
}
declare const AutocompleteRoot: <T extends object = object, M extends "single" | "multiple" = "single">({ children, className, fullWidth, isDisabled, onClear, variant, ...props }: AutocompleteRootProps<T, M>) => import("react/jsx-runtime").JSX.Element;
interface AutocompleteTriggerProps extends ComponentPropsWithRef<typeof GroupPrimitive> {
}
declare const AutocompleteTrigger: {
    ({ children, className, isDisabled: isDisabledProp, onClick, ref, ...props }: AutocompleteTriggerProps): import("react/jsx-runtime").JSX.Element | null;
    displayName: string;
};
interface AutocompleteValueProps extends ComponentPropsWithRef<typeof SelectValuePrimitive> {
}
declare const AutocompleteValue: ({ children, className, ...props }: AutocompleteValueProps) => import("react/jsx-runtime").JSX.Element;
interface AutocompleteIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "svg"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AutocompleteIndicator: <E extends keyof React.JSX.IntrinsicElements = "svg">({ children, className, ...props }: AutocompleteIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AutocompleteIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface AutocompletePopoverProps extends Omit<ComponentPropsWithRef<typeof PopoverPrimitive>, "children"> {
    children: React.ReactNode;
}
declare const AutocompletePopover: ({ children, className, placement, style, ...props }: AutocompletePopoverProps) => import("react/jsx-runtime").JSX.Element;
interface AutocompleteFilterProps extends ComponentPropsWithRef<typeof AutocompletePrimitive> {
}
declare const AutocompleteFilter: ({ children, ...props }: AutocompleteFilterProps) => import("react/jsx-runtime").JSX.Element;
interface AutocompleteClearButtonProps<E extends keyof React.JSX.IntrinsicElements = "button"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const AutocompleteClearButton: <E extends keyof React.JSX.IntrinsicElements = "button">({ className, onClick, ref, ...props }: AutocompleteClearButtonProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof AutocompleteClearButtonProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { AutocompleteRoot, AutocompleteTrigger, AutocompleteValue, AutocompleteIndicator, AutocompletePopover, AutocompleteFilter, AutocompleteClearButton, };
export type { AutocompleteRootProps, AutocompleteTriggerProps, AutocompleteValueProps, AutocompleteIndicatorProps, AutocompletePopoverProps, AutocompleteFilterProps, AutocompleteClearButtonProps, };
