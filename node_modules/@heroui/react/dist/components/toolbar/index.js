"use strict";
import { ToolbarRoot } from './toolbar.js';
export { toolbarVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Toolbar = Object.assign(ToolbarRoot, {
  Root: ToolbarRoot
});

export { Toolbar, ToolbarRoot };
