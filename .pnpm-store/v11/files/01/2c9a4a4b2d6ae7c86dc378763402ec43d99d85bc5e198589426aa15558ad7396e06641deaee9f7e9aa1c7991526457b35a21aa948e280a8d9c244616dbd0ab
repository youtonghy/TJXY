import type { DOMRenderProps } from "../../utils/dom";
import type { DateInputGroupVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { DateInputProps as DateInputPrimitiveProps, DateSegmentProps as DateSegmentPrimitiveProps, DateInputProps as TimeInputPrimitiveProps, DateSegmentProps as TimeSegmentPrimitiveProps } from "react-aria-components/DateField";
import React from "react";
import { Group as GroupPrimitive } from "react-aria-components/Group";
interface DateInputGroupRootProps extends ComponentPropsWithRef<typeof GroupPrimitive>, DateInputGroupVariants {
}
declare const DateInputGroupRoot: ({ children, className, fullWidth, variant, ...props }: DateInputGroupRootProps) => import("react/jsx-runtime").JSX.Element;
interface DateInputGroupPrefixProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DateInputGroupPrefix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DateInputGroupPrefixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DateInputGroupPrefixProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface DateInputGroupInputProps extends DateInputPrimitiveProps, Partial<Omit<TimeInputPrimitiveProps, keyof DateInputPrimitiveProps>> {
}
declare const DateInputGroupInput: ({ className, ...props }: DateInputGroupInputProps) => import("react/jsx-runtime").JSX.Element;
interface DateInputGroupSegmentProps extends DateSegmentPrimitiveProps, Partial<Omit<TimeSegmentPrimitiveProps, keyof DateSegmentPrimitiveProps>> {
    className?: string;
}
declare const DateInputGroupSegment: ({ className, segment, ...props }: DateInputGroupSegmentProps) => import("react/jsx-runtime").JSX.Element;
interface DateInputGroupInputContainerProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DateInputGroupInputContainer: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DateInputGroupInputContainerProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DateInputGroupInputContainerProps<E>>) => import("react/jsx-runtime").JSX.Element;
interface DateInputGroupSuffixProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined> {
    children?: ReactNode;
    className?: string;
}
declare const DateInputGroupSuffix: <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, ...props }: DateInputGroupSuffixProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof DateInputGroupSuffixProps<E>>) => import("react/jsx-runtime").JSX.Element;
export { DateInputGroupRoot, DateInputGroupInput, DateInputGroupInputContainer, DateInputGroupSegment, DateInputGroupPrefix, DateInputGroupSuffix, };
export type { DateInputGroupRootProps, DateInputGroupInputProps, DateInputGroupInputContainerProps, DateInputGroupSegmentProps, DateInputGroupPrefixProps, DateInputGroupSuffixProps, };
