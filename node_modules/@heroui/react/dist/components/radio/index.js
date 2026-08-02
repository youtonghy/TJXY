"use strict";
import { RadioRoot, RadioIndicator, RadioControl, RadioContent } from './radio.js';
export { radioVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Radio = Object.assign(RadioRoot, {
  Root: RadioRoot,
  Content: RadioContent,
  Control: RadioControl,
  Indicator: RadioIndicator
});

export { Radio, RadioContent, RadioControl, RadioIndicator, RadioRoot };
