"use strict";
import { RadioGroupRoot } from './radio-group.js';
export { radioGroupVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const RadioGroup = Object.assign(RadioGroupRoot, {
  Root: RadioGroupRoot
});

export { RadioGroup, RadioGroupRoot };
