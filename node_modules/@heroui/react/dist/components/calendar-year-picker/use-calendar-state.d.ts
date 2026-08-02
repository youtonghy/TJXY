/**
 * Returns the active Calendar or RangeCalendar state from RAC context.
 * Must be used inside <Calendar> or <RangeCalendar>.
 */
declare function useCalendarOrRangeState(): import("react-aria-components").CalendarState<import("react-aria-components/Calendar").CalendarSelectionMode> | import("react-aria-components").RangeCalendarState<import("react-aria/useCalendar").DateValue>;
export { useCalendarOrRangeState };
