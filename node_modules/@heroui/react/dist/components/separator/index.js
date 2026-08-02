"use strict";
import { SeparatorRoot } from './separator.js';
export { separatorVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Separator = Object.assign(SeparatorRoot, {
  Root: SeparatorRoot
});

export { Separator, SeparatorRoot };
