"use strict";
import { AutocompleteRoot, AutocompleteClearButton, AutocompleteFilter, AutocompletePopover, AutocompleteIndicator, AutocompleteValue, AutocompleteTrigger } from './autocomplete.js';
export { autocompleteVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const Autocomplete = Object.assign(AutocompleteRoot, {
  Root: AutocompleteRoot,
  Trigger: AutocompleteTrigger,
  Value: AutocompleteValue,
  Indicator: AutocompleteIndicator,
  Popover: AutocompletePopover,
  Filter: AutocompleteFilter,
  ClearButton: AutocompleteClearButton
});

export { Autocomplete, AutocompleteClearButton, AutocompleteFilter, AutocompleteIndicator, AutocompletePopover, AutocompleteRoot, AutocompleteTrigger, AutocompleteValue };
