"use strict";
import { CalendarYearPickerCell, CalendarYearPickerGridBody, CalendarYearPickerGrid, CalendarYearPickerTriggerIndicator, CalendarYearPickerTriggerHeading, CalendarYearPickerTrigger } from '../calendar-year-picker/calendar-year-picker.js';
export { YearPickerContext, useYearPicker } from '../calendar-year-picker/year-picker-context.js';
export { useCalendarOrRangeState } from '../calendar-year-picker/use-calendar-state.js';
export { rangeCalendarVariants } from '@heroui/styles';
import { RangeCalendarRoot, RangeCalendarCellIndicator, RangeCalendarCell, RangeCalendarHeaderCell, RangeCalendarGridBody, RangeCalendarGridHeader, RangeCalendarGrid, RangeCalendarNavButton, RangeCalendarHeading, RangeCalendarHeader } from './range-calendar.js';

/* -------------------------------------------------------------------------------------------------
| * Compound Component
| * -----------------------------------------------------------------------------------------------*/
const RangeCalendar = Object.assign(RangeCalendarRoot, {
  Root: RangeCalendarRoot,
  Header: RangeCalendarHeader,
  Heading: RangeCalendarHeading,
  NavButton: RangeCalendarNavButton,
  Grid: RangeCalendarGrid,
  GridHeader: RangeCalendarGridHeader,
  GridBody: RangeCalendarGridBody,
  HeaderCell: RangeCalendarHeaderCell,
  Cell: RangeCalendarCell,
  CellIndicator: RangeCalendarCellIndicator,
  YearPickerTrigger: CalendarYearPickerTrigger,
  YearPickerTriggerHeading: CalendarYearPickerTriggerHeading,
  YearPickerTriggerIndicator: CalendarYearPickerTriggerIndicator,
  YearPickerGrid: CalendarYearPickerGrid,
  YearPickerGridBody: CalendarYearPickerGridBody,
  YearPickerCell: CalendarYearPickerCell
});

export { RangeCalendar, RangeCalendarCell, RangeCalendarCellIndicator, RangeCalendarGrid, RangeCalendarGridBody, RangeCalendarGridHeader, RangeCalendarHeader, RangeCalendarHeaderCell, RangeCalendarHeading, RangeCalendarNavButton, RangeCalendarRoot };
