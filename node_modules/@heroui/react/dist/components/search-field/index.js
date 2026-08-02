"use strict";
import { SearchFieldRoot, SearchFieldClearButton, SearchFieldSearchIcon, SearchFieldInput, SearchFieldGroup } from './search-field.js';
export { searchFieldVariants } from '@heroui/styles';

/* -------------------------------------------------------------------------------------------------
 * Compound Component
 * -----------------------------------------------------------------------------------------------*/
const SearchField = Object.assign(SearchFieldRoot, {
  Root: SearchFieldRoot,
  Group: SearchFieldGroup,
  Input: SearchFieldInput,
  SearchIcon: SearchFieldSearchIcon,
  ClearButton: SearchFieldClearButton
});

export { SearchField, SearchFieldClearButton, SearchFieldGroup, SearchFieldInput, SearchFieldRoot, SearchFieldSearchIcon };
