"use strict";
import { SelectRoot, SelectPopover, SelectIndicator, SelectValue, SelectTrigger } from './select.js';
export { selectVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Select = Object.assign(SelectRoot, {
  Root: SelectRoot,
  Trigger: SelectTrigger,
  Value: SelectValue,
  Indicator: SelectIndicator,
  Popover: SelectPopover
});

export { Select, SelectIndicator, SelectPopover, SelectRoot, SelectTrigger, SelectValue };
