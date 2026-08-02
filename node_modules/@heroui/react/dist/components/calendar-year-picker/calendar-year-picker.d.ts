import type { DOMRenderProps } from "../../utils/dom";
import type { CalendarYearPickerVariants } from "@heroui/styles";
import type { ComponentPropsWithRef, ReactNode } from "react";
import type { CalendarHeadingProps, CalendarYearPickerProps } from "react-aria/useCalendar";
import React from "react";
import { Button as ButtonPrimitive } from "react-aria-components/Button";
interface CalendarYearPickerTriggerProps extends Omit<ComponentPropsWithRef<typeof ButtonPrimitive>, "children">, CalendarYearPickerVariants {
    children: React.ReactNode | ((values: CalendarYearPickerTriggerRenderProps) => React.ReactNode);
}
interface CalendarYearPickerTriggerRenderProps {
    isOpen: boolean;
    monthYear: string;
    toggle: () => void;
}
interface CalendarYearPickerTriggerHeadingProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined>, CalendarYearPickerVariants, Pick<CalendarHeadingProps, "offset" | "format"> {
    children?: ReactNode | ((values: CalendarYearPickerTriggerRenderProps) => ReactNode);
    className?: string;
}
interface CalendarYearPickerTriggerIndicatorProps<E extends keyof React.JSX.IntrinsicElements = "span"> extends DOMRenderProps<E, undefined>, CalendarYearPickerVariants {
    children?: ReactNode | ((values: CalendarYearPickerTriggerRenderProps) => ReactNode);
    className?: string;
}
declare const CalendarYearPickerTrigger: {
    ({ children, className, onKeyDown, onPress, ...props }: CalendarYearPickerTriggerProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
declare const CalendarYearPickerTriggerHeading: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, format, offset, ...props }: CalendarYearPickerTriggerHeadingProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CalendarYearPickerTriggerHeadingProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
declare const CalendarYearPickerTriggerIndicator: {
    <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: CalendarYearPickerTriggerIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CalendarYearPickerTriggerIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
interface CalendarYearPickerGridProps<E extends keyof React.JSX.IntrinsicElements = "div"> extends DOMRenderProps<E, undefined>, CalendarYearPickerVariants, Pick<CalendarYearPickerProps, "format" | "visibleYears"> {
    children?: ReactNode;
    className?: string;
}
interface CalendarYearPickerCellRenderProps {
    year: number;
    formattedYear: string;
    isSelected: boolean;
    isCurrentYear: boolean;
    isOpen: boolean;
    selectYear: () => void;
}
interface CalendarYearPickerGridBodyProps {
    children?: (values: CalendarYearPickerCellRenderProps) => React.ReactNode;
}
interface CalendarYearPickerCellProps extends Omit<ComponentPropsWithRef<typeof ButtonPrimitive>, "children">, CalendarYearPickerVariants {
    year: number;
    children?: React.ReactNode | ((values: CalendarYearPickerCellRenderProps) => React.ReactNode);
}
declare const CalendarYearPickerGrid: {
    <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, format, onKeyDown, visibleYears: visibleYearsProp, ...props }: CalendarYearPickerGridProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof CalendarYearPickerGridProps<E>>): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
declare const CalendarYearPickerGridBody: {
    ({ children }: CalendarYearPickerGridBodyProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
declare const CalendarYearPickerCell: {
    ({ children, className, excludeFromTabOrder, onFocus, onPress, year, ...props }: CalendarYearPickerCellProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
};
export { CalendarYearPickerTrigger, CalendarYearPickerTriggerHeading, CalendarYearPickerTriggerIndicator, CalendarYearPickerGrid, CalendarYearPickerGridBody, CalendarYearPickerCell, };
export type { CalendarYearPickerTriggerProps, CalendarYearPickerTriggerHeadingProps, CalendarYearPickerTriggerIndicatorProps, CalendarYearPickerTriggerRenderProps, CalendarYearPickerGridProps, CalendarYearPickerGridBodyProps, CalendarYearPickerCellProps, CalendarYearPickerCellRenderProps, };
