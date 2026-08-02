import type { CalendarRootProps } from "./calendar";
import type { ComponentProps, ReactElement } from "react";
import type { CalendarSelectionMode, DateValue } from "react-aria-components/Calendar";
import { CalendarYearPickerCell, CalendarYearPickerGrid, CalendarYearPickerGridBody, CalendarYearPickerTrigger, CalendarYearPickerTriggerHeading, CalendarYearPickerTriggerIndicator } from "../calendar-year-picker";
import { CalendarCell, CalendarCellIndicator, CalendarGrid, CalendarGridBody, CalendarGridHeader, CalendarHeader, CalendarHeaderCell, CalendarHeading, CalendarNavButton, CalendarRoot } from "./calendar";
type CalendarComponent = <T extends DateValue = DateValue, M extends CalendarSelectionMode = "single">(props: CalendarRootProps<T, M>) => ReactElement | null;
declare const CalendarCompound: typeof CalendarRoot & {
    Root: typeof CalendarRoot;
    Header: {
        <E extends keyof React.JSX.IntrinsicElements = "header">({ children, className, ...props }: import("./calendar").CalendarHeaderProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./calendar").CalendarHeaderProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Heading: {
        ({ className, ...props }: import("./calendar").CalendarHeadingProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    NavButton: {
        ({ children, className, slot, ...props }: import("./calendar").CalendarNavButtonProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Grid: {
        ({ children, className, weekdayStyle, ...props }: import("./calendar").CalendarGridProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    GridHeader: {
        ({ children, className, ...props }: import("./calendar").CalendarGridHeaderProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    GridBody: {
        ({ children, className, ...props }: import("./calendar").CalendarGridBodyProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    HeaderCell: {
        ({ className, ...props }: import("./calendar").CalendarHeaderCellProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Cell: {
        ({ children, className, ...props }: import("./calendar").CalendarCellProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    CellIndicator: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ className, ...props }: import("./calendar").CalendarCellIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./calendar").CalendarCellIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    YearPickerTrigger: {
        ({ children, className, onKeyDown, onPress, ...props }: import("..").CalendarYearPickerTriggerProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    YearPickerTriggerHeading: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, format, offset, ...props }: import("..").CalendarYearPickerTriggerHeadingProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("..").CalendarYearPickerTriggerHeadingProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    YearPickerTriggerIndicator: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("..").CalendarYearPickerTriggerIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("..").CalendarYearPickerTriggerIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    YearPickerGrid: {
        <E extends keyof React.JSX.IntrinsicElements = "div">({ children, className, format, onKeyDown, visibleYears: visibleYearsProp, ...props }: import("..").CalendarYearPickerGridProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("..").CalendarYearPickerGridProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    YearPickerGridBody: {
        ({ children }: import("..").CalendarYearPickerGridBodyProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    YearPickerCell: {
        ({ children, className, excludeFromTabOrder, onFocus, onPress, year, ...props }: import("..").CalendarYearPickerCellProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export declare const Calendar: CalendarComponent & typeof CalendarCompound;
export type Calendar = {
    Props: ComponentProps<typeof CalendarRoot>;
    RootProps: ComponentProps<typeof CalendarRoot>;
    HeaderProps: ComponentProps<typeof CalendarHeader>;
    HeadingProps: ComponentProps<typeof CalendarHeading>;
    NavButtonProps: ComponentProps<typeof CalendarNavButton>;
    GridProps: ComponentProps<typeof CalendarGrid>;
    GridHeaderProps: ComponentProps<typeof CalendarGridHeader>;
    GridBodyProps: ComponentProps<typeof CalendarGridBody>;
    HeaderCellProps: ComponentProps<typeof CalendarHeaderCell>;
    CellProps: ComponentProps<typeof CalendarCell>;
    CellIndicatorProps: ComponentProps<typeof CalendarCellIndicator>;
    YearPickerTriggerProps: ComponentProps<typeof CalendarYearPickerTrigger>;
    YearPickerTriggerHeadingProps: ComponentProps<typeof CalendarYearPickerTriggerHeading>;
    YearPickerTriggerIndicatorProps: ComponentProps<typeof CalendarYearPickerTriggerIndicator>;
    YearPickerGridProps: ComponentProps<typeof CalendarYearPickerGrid>;
    YearPickerGridBodyProps: ComponentProps<typeof CalendarYearPickerGridBody>;
    YearPickerCellProps: ComponentProps<typeof CalendarYearPickerCell>;
};
export { CalendarRoot, CalendarHeader, CalendarHeading, CalendarNavButton, CalendarGrid, CalendarGridHeader, CalendarGridBody, CalendarHeaderCell, CalendarCell, CalendarCellIndicator, };
export type { CalendarRootProps, CalendarRootProps as CalendarProps, CalendarHeaderProps, CalendarHeadingProps, CalendarNavButtonProps, CalendarGridProps, CalendarGridHeaderProps, CalendarGridBodyProps, CalendarHeaderCellProps, CalendarCellProps, CalendarCellIndicatorProps, } from "./calendar";
export { YearPickerContext, useYearPicker, useCalendarOrRangeState } from "../calendar-year-picker";
export type { YearPickerContextValue } from "../calendar-year-picker";
export { calendarVariants } from "@heroui/styles";
export type { CalendarVariants } from "@heroui/styles";
export type { CalendarSelectionMode } from "react-aria-components/Calendar";
