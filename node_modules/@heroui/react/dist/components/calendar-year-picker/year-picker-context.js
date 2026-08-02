"use strict";
import { createContext, use } from 'react';

/* -------------------------------------------------------------------------------------------------
 * YearPickerContext
 *
 * Context provided by Calendar (and RangeCalendar) to control year-picker visibility.
 * Internal child components consume this to toggle the year-picker open/close state,
 * keeping the public API clean and wrapper-free.
 * -----------------------------------------------------------------------------------------------*/

const YearPickerContext = /*#__PURE__*/createContext(null);

/**
 * Hook to consume YearPickerContext.
 * Must be used inside Calendar or RangeCalendar.
 */
function useYearPicker() {
  const context = use(YearPickerContext);
  if (!context) {
    throw new Error("useYearPicker must be used within a <Calendar> or <RangeCalendar> component.");
  }
  return context;
}

export { YearPickerContext, useYearPicker };
