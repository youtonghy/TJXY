"use strict";
import { TextAreaRoot } from './textarea.js';
export { textAreaVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const TextArea = Object.assign(TextAreaRoot, {
  Root: TextAreaRoot
});

export { TextArea, TextAreaRoot };
