"use strict";
import { FormRoot } from './form.js';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Form = Object.assign(FormRoot, {
  Root: FormRoot
});

export { Form, FormRoot };
