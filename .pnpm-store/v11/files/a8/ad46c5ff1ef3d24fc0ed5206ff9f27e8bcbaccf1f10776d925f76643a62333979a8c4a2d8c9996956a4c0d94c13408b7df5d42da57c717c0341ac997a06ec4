interface YearPickerContextValue {
    isYearPickerOpen: boolean;
    setIsYearPickerOpen: (open: boolean) => void;
    calendarRef: React.RefObject<HTMLDivElement | null>;
    calendarGridSlot: "calendar-grid" | "range-calendar-grid";
}
declare const YearPickerContext: import("react").Context<YearPickerContextValue | null>;
/**
 * Hook to consume YearPickerContext.
 * Must be used inside Calendar or RangeCalendar.
 */
declare function useYearPicker(): YearPickerContextValue;
export { YearPickerContext, useYearPicker };
export type { YearPickerContextValue };
