"use strict";
import { DescriptionRoot } from './description.js';
export { descriptionVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Description = Object.assign(DescriptionRoot, {
  Root: DescriptionRoot
});

export { Description, DescriptionRoot };
