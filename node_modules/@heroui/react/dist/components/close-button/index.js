"use strict";
import { CloseButtonRoot } from './close-button.js';
export { closeButtonVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const CloseButton = Object.assign(CloseButtonRoot, {
  Root: CloseButtonRoot
});

export { CloseButton, CloseButtonRoot };
