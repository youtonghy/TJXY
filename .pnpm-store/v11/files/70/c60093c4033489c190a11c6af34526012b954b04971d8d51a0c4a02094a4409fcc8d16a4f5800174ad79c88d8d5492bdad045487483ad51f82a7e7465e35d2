import type { DOMRenderProps } from "../../utils/dom";
import type { SelectVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import React from "react";
import { Button as ButtonPrimitive } from "react-aria-components/Button";
import { Popover as PopoverPrimitive } from "react-aria-components/Popover";
import { Select as SelectPrimitive, SelectValue as SelectValuePrimitive } from "react-aria-components/Select";
interface SelectRootProps<T extends object, M extends "single" | "multiple" = "single"> extends ComponentPropsWithRef<typeof SelectPrimitive<T, M>>, SelectVariants {
    items?: Iterable<T, M>;
}
declare const SelectRoot: <T extends object = object, M extends "single" | "multiple" = "single">({ children, className, fullWidth, variant, ...props }: SelectRootProps<T, M>) => import("react/jsx-runtime").JSX.Element;
interface SelectTriggerProps extends ComponentPropsWithRef<typeof ButtonPrimitive> {
}
declare const SelectTrigger: ({ children, className, ...props }: SelectTriggerProps) => import("react/jsx-runtime").JSX.Element;
interface SelectValueProps extends ComponentPropsWithRef<typeof SelectValuePrimitive> {
}
declare const SelectValue: ({ children, className, ...props }: SelectValueProps) => import("react/jsx-runtime").JSX.Element;
interface SelectIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "svg"> extends DOMRenderProps<E, undefined> {
    children?: React.ReactNode;
    className?: string;
}
declare const SelectIndicator: <E extends keyof React.JSX.IntrinsicElements = "svg">({ children, className, ...props }: SelectIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof SelectIndicatorProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface SelectPopoverProps extends Omit<ComponentPropsWithRef<typeof PopoverPrimitive>, "children"> {
    children: React.ReactNode;
}
declare const SelectPopover: ({ children, className, placement, ...props }: SelectPopoverProps) => import("react/jsx-runtime").JSX.Element;
export { SelectRoot, SelectTrigger, SelectValue, SelectIndicator, SelectPopover };
export type { SelectRootProps, SelectTriggerProps, SelectValueProps, SelectIndicatorProps, SelectPopoverProps, };
