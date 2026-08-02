"use strict";
import { ToggleButtonRoot } from './toggle-button.js';
export { toggleButtonVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const ToggleButton = Object.assign(ToggleButtonRoot, {
  Root: ToggleButtonRoot
});

export { ToggleButton, ToggleButtonRoot };
