"use strict";
import '../date-input-group/index.js';
import { DateFieldRoot } from './date-field.js';
export { dateFieldVariants } from '@heroui/styles';
import { DateInputGroupSuffix, DateInputGroupPrefix, DateInputGroupSegment, DateInputGroupInputContainer, DateInputGroupInput, DateInputGroupRoot } from '../date-input-group/date-input-group.js';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const DateField = Object.assign(DateFieldRoot, {
  Root: DateFieldRoot,
  Group: DateInputGroupRoot,
  Input: DateInputGroupInput,
  InputContainer: DateInputGroupInputContainer,
  Segment: DateInputGroupSegment,
  Prefix: DateInputGroupPrefix,
  Suffix: DateInputGroupSuffix
});

export { DateField, DateFieldRoot };
