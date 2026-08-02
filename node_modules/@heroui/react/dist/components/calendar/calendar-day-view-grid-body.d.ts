import type { CalendarDate, DateValue } from "@internationalized/date";
import type { ReactElement } from "react";
interface CalendarDayViewGridBodyProps {
    children: (date: CalendarDate) => ReactElement;
    className?: string;
    "data-slot"?: string;
    firstDayOfWeek?: "sun" | "mon" | "tue" | "wed" | "thu" | "fri" | "sat";
    visibleRange: {
        end: DateValue;
        start: DateValue;
    };
}
export declare function CalendarDayViewGridBody({ children, className, "data-slot": dataSlot, firstDayOfWeek, visibleRange, }: CalendarDayViewGridBodyProps): import("react/jsx-runtime").JSX.Element;
export {};
