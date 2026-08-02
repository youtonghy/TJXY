"use strict";
import { LabelRoot } from './label.js';
export { labelVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Label = Object.assign(LabelRoot, {
  Root: LabelRoot
});

export { Label, LabelRoot };
