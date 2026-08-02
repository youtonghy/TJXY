"use strict";
import { CheckboxRoot, CheckboxIndicator, CheckboxControl, CheckboxContent } from './checkbox.js';
export { checkboxVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Checkbox = Object.assign(CheckboxRoot, {
  Root: CheckboxRoot,
  Content: CheckboxContent,
  Control: CheckboxControl,
  Indicator: CheckboxIndicator
});

export { Checkbox, CheckboxContent, CheckboxControl, CheckboxIndicator, CheckboxRoot };
