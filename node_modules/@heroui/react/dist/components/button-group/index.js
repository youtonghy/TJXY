"use strict";
import { ButtonGroupRoot, ButtonGroupSeparator } from './button-group.js';
export { BUTTON_GROUP_CHILD, ButtonGroupContext } from './button-group.js';
export { buttonGroupVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const ButtonGroup = Object.assign(ButtonGroupRoot, {
  Root: ButtonGroupRoot,
  Separator: ButtonGroupSeparator
});

export { ButtonGroup, ButtonGroupRoot, ButtonGroupSeparator };
