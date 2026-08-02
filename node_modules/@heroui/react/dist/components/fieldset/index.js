"use strict";
import { FieldsetRoot, FieldsetActions, FieldGroup, FieldsetLegend } from './fieldset.js';
export { fieldsetVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Fieldset = Object.assign(FieldsetRoot, {
  Root: FieldsetRoot,
  Legend: FieldsetLegend,
  Group: FieldGroup,
  Actions: FieldsetActions
});

export { FieldGroup, Fieldset, FieldsetActions, FieldsetLegend, FieldsetRoot };
