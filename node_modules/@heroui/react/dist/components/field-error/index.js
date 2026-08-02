"use strict";
import { FieldErrorRoot } from './field-error.js';
export { fieldErrorVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const FieldError = Object.assign(FieldErrorRoot, {
  Root: FieldErrorRoot
});

export { FieldError, FieldErrorRoot };
