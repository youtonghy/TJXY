"use strict";
import '../date-input-group/index.js';
import { TimeFieldRoot } from './time-field.js';
export { timeFieldVariants } from '@heroui/styles';
import { DateInputGroupSuffix, DateInputGroupPrefix, DateInputGroupSegment, DateInputGroupInputContainer, DateInputGroupInput, DateInputGroupRoot } from '../date-input-group/date-input-group.js';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const TimeField = Object.assign(TimeFieldRoot, {
  Root: TimeFieldRoot,
  Group: DateInputGroupRoot,
  Input: DateInputGroupInput,
  InputContainer: DateInputGroupInputContainer,
  Segment: DateInputGroupSegment,
  Prefix: DateInputGroupPrefix,
  Suffix: DateInputGroupSuffix
});

export { TimeField, TimeFieldRoot };
