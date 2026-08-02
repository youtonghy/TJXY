"use strict";
import { TextFieldRoot } from './textfield.js';
export { TextFieldContext } from './textfield.js';
export { textFieldVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const TextField = Object.assign(TextFieldRoot, {
  Root: TextFieldRoot
});

export { TextField, TextFieldRoot };
