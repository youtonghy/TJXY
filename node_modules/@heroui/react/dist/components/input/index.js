"use strict";
import { InputRoot } from './input.js';
export { inputVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Input = Object.assign(InputRoot, {
  Root: InputRoot
});

export { Input, InputRoot };
