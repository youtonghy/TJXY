"use strict";
import { DateRangePickerRoot, DateRangePickerPopover, DateRangePickerRangeSeparator, DateRangePickerTriggerIndicator, DateRangePickerTrigger } from './date-range-picker.js';
export { dateRangePickerVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const DateRangePicker = Object.assign(DateRangePickerRoot, {
  Root: DateRangePickerRoot,
  Trigger: DateRangePickerTrigger,
  TriggerIndicator: DateRangePickerTriggerIndicator,
  RangeSeparator: DateRangePickerRangeSeparator,
  Popover: DateRangePickerPopover
});

export { DateRangePicker, DateRangePickerPopover, DateRangePickerRangeSeparator, DateRangePickerRoot, DateRangePickerTrigger, DateRangePickerTriggerIndicator };
