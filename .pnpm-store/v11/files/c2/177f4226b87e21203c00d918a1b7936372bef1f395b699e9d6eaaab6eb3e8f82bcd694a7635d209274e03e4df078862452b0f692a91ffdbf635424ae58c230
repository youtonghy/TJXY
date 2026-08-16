"use client";
import { calendarYearPickerVariants } from '@heroui/styles';
import React__default from 'react';
import { useCalendarYearPicker, useCalendarHeading } from 'react-aria/useCalendar';
import { Button } from 'react-aria-components/Button';
import { getYearRange } from '../../utils/calendar.js';
import { composeTwRenderProps, composeSlotClassName } from '../../utils/compose.js';
import { dom } from '../../utils/dom.js';
import { IconChevronRight } from '../icons.js';
import { useCalendarOrRangeState } from './use-calendar-state.js';
import { useYearPicker } from './year-picker-context.js';
import { jsx, Fragment } from 'react/jsx-runtime';

const CalendarYearPickerTriggerContext = /*#__PURE__*/React__default.createContext(null);
function useCalendarYearPickerTriggerContext() {
  const context = React__default.use(CalendarYearPickerTriggerContext);
  if (!context) {
    throw new Error("CalendarYearPicker trigger components must be used within <CalendarYearPicker.Trigger>.");
  }
  return context;
}
const CalendarYearPickerTrigger = ({
  children,
  className,
  onKeyDown,
  onPress,
  ...props
}) => {
  const {
    isYearPickerOpen,
    setIsYearPickerOpen
  } = useYearPicker();
  const state = useCalendarOrRangeState();
  const monthYear = useCalendarHeading({}, state);
  const slots = React__default.useMemo(() => calendarYearPickerVariants(), []);
  const handleToggle = React__default.useCallback(() => {
    setIsYearPickerOpen(!isYearPickerOpen);
  }, [isYearPickerOpen, setIsYearPickerOpen]);
  const handleKeyDown = e => {
    onKeyDown?.(e);
    if (e.defaultPrevented) {
      return;
    }
    if (e.key === "Escape" && isYearPickerOpen) {
      e.preventDefault();
      setIsYearPickerOpen(false);
    }
  };
  const values = React__default.useMemo(() => ({
    isOpen: isYearPickerOpen,
    monthYear,
    toggle: handleToggle
  }), [handleToggle, isYearPickerOpen, monthYear]);
  const contextValue = React__default.useMemo(() => ({
    ...values,
    slots
  }), [slots, values]);
  return /*#__PURE__*/jsx(CalendarYearPickerTriggerContext, {
    value: contextValue,
    children: /*#__PURE__*/jsx(Button, {
      "aria-expanded": isYearPickerOpen,
      "aria-label": `${monthYear}, year selector`,
      className: composeTwRenderProps(className, slots.trigger()),
      "data-open": isYearPickerOpen || undefined,
      "data-slot": "calendar-year-picker-trigger",
      slot: null,
      onKeyDown: handleKeyDown,
      onPress: event => {
        onPress?.(event);
        handleToggle();
      },
      ...props,
      children: typeof children === "function" ? children(values) : children
    })
  });
};
CalendarYearPickerTrigger.displayName = "HeroUI.CalendarYearPicker.Trigger";

/* -------------------------------------------------------------------------------------------------
 * CalendarYearPickerTriggerHeading
 * -----------------------------------------------------------------------------------------------*/
const CalendarYearPickerTriggerHeading = ({
  children,
  className,
  format,
  offset,
  ...props
}) => {
  const {
    monthYear,
    slots,
    ...values
  } = useCalendarYearPickerTriggerContext();
  const state = useCalendarOrRangeState();
  const heading = useCalendarHeading({
    format,
    offset
  }, state);
  return /*#__PURE__*/jsx(dom.span, {
    className: composeSlotClassName(slots.triggerHeading, className),
    "data-slot": "calendar-year-picker-trigger-heading",
    ...props,
    children: typeof children === "function" ? children({
      monthYear,
      ...values
    }) : children || heading
  });
};
CalendarYearPickerTriggerHeading.displayName = "HeroUI.CalendarYearPicker.TriggerHeading";

/* -------------------------------------------------------------------------------------------------
 * CalendarYearPickerTriggerIndicator
 * -----------------------------------------------------------------------------------------------*/
const CalendarYearPickerTriggerIndicator = ({
  children,
  className,
  ...props
}) => {
  const {
    monthYear,
    slots,
    ...values
  } = useCalendarYearPickerTriggerContext();
  return /*#__PURE__*/jsx(dom.span, {
    "aria-hidden": "true",
    className: composeSlotClassName(slots.triggerIndicator, className),
    "data-slot": "calendar-year-picker-trigger-indicator",
    ...props,
    children: typeof children === "function" ? children({
      monthYear,
      ...values
    }) : children || /*#__PURE__*/jsx(IconChevronRight, {
      height: "1em",
      width: "1em"
    })
  });
};
CalendarYearPickerTriggerIndicator.displayName = "HeroUI.CalendarYearPicker.TriggerIndicator";

/* -------------------------------------------------------------------------------------------------
 * CalendarYearPickerGrid
 *
 * Renders a 3-column grid of year buttons. Hidden via CSS opacity when closed,
 * visible when data-open="true".  tabIndex is toggled so only the active view
 * receives keyboard focus.
 * -----------------------------------------------------------------------------------------------*/

const CalendarYearPickerGridContext = /*#__PURE__*/React__default.createContext(null);
function useCalendarYearPickerGridContext() {
  const context = React__default.use(CalendarYearPickerGridContext);
  if (!context) {
    throw new Error("CalendarYearPicker components must be used within <CalendarYearPicker.Grid>.");
  }
  return context;
}
const CalendarYearPickerGrid = ({
  children,
  className,
  format,
  onKeyDown,
  visibleYears: visibleYearsProp,
  ...props
}) => {
  const {
    calendarGridSlot,
    calendarRef,
    isYearPickerOpen,
    setIsYearPickerOpen
  } = useYearPicker();
  const state = useCalendarOrRangeState();
  const gridRef = React__default.useRef(null);
  const slots = React__default.useMemo(() => calendarYearPickerVariants(), []);
  const visibleYears = React__default.useMemo(() => {
    if (visibleYearsProp != null) {
      return visibleYearsProp;
    }
    if (!state.minValue || !state.maxValue) {
      return 20;
    }
    return getYearRange(state.minValue, state.maxValue).length;
  }, [state.maxValue, state.minValue, visibleYearsProp]);
  const {
    "aria-label": yearGridAriaLabel,
    items,
    onChange,
    value: focusedItemId
  } = useCalendarYearPicker({
    format,
    visibleYears
  }, state);

  // useCalendarYearPicker returns a new `items` array every render — derive a stable
  // key so effects don't re-run and steal focus back to the selected year.
  const itemsKey = items.map(item => `${item.id}:${item.date.year}`).join("|");
  const years = items.map(item => item.date.year);
  const itemByYear = new Map(items.map(item => [item.date.year, item]));
  const focusedYear = items[focusedItemId]?.date.year ?? state.focusedDate.year;
  const getFormattedYear = React__default.useCallback(year => itemByYear.get(year)?.formatted ?? String(year), [itemByYear]);
  const [activeYear, setActiveYear] = React__default.useState(focusedYear);
  const wasYearPickerOpenRef = React__default.useRef(false);

  // Position the year grid to overlay the day grid
  React__default.useEffect(() => {
    const yearGrid = gridRef.current;
    if (!yearGrid) return;
    const calendar = calendarRef.current;
    const calendarGrid = calendar?.querySelector(`[data-slot='${calendarGridSlot}']`);
    if (calendarGrid) {
      yearGrid.style.top = `${calendarGrid.offsetTop}px`;
      yearGrid.style.height = `${calendarGrid.offsetHeight}px`;
    }
  }, [calendarGridSlot, calendarRef, state.focusedDate]);
  const focusYearCell = React__default.useCallback(year => {
    const yearGrid = gridRef.current;
    if (!yearGrid) return;
    const yearCell = yearGrid.querySelector(`[data-year='${year}']`);
    if (yearCell) {
      yearCell.focus();
    }
  }, []);

  // Anchor keyboard focus to the selected year only when the picker opens — not on
  // every render while open (items from useCalendarYearPicker is unstable).
  React__default.useEffect(() => {
    const justOpened = isYearPickerOpen && !wasYearPickerOpenRef.current;
    wasYearPickerOpenRef.current = isYearPickerOpen;
    if (!justOpened || years.length === 0) return;
    const [firstYear] = years;
    if (firstYear == null) return;
    const nextActiveYear = years.includes(focusedYear) ? focusedYear : firstYear;
    setActiveYear(nextActiveYear);
    const rafId = requestAnimationFrame(() => {
      focusYearCell(nextActiveYear);
    });
    return () => {
      cancelAnimationFrame(rafId);
    };
  }, [focusYearCell, focusedYear, isYearPickerOpen, itemsKey, years.length]);
  React__default.useEffect(() => {
    if (!isYearPickerOpen || years.length === 0) return;
    const [firstYear] = years;
    if (firstYear == null) return;
    if (!years.includes(activeYear)) {
      setActiveYear(firstYear);
    }
  }, [activeYear, isYearPickerOpen, itemsKey, years]);
  const handleYearSelect = React__default.useCallback(year => {
    const item = itemByYear.get(year);
    if (item == null) {
      return;
    }
    setIsYearPickerOpen(false);
    onChange(item.id);
  }, [itemByYear, onChange, setIsYearPickerOpen]);
  const handleKeyDown = e => {
    onKeyDown?.(e);
    if (e.defaultPrevented) {
      return;
    }
    if (e.key === "Escape" && isYearPickerOpen) {
      e.preventDefault();
      setIsYearPickerOpen(false);
      return;
    }
    if (!isYearPickerOpen || years.length === 0) {
      return;
    }
    const currentIndex = years.indexOf(activeYear);
    if (currentIndex === -1) {
      return;
    }
    let nextIndex = currentIndex;
    switch (e.key) {
      case "ArrowRight":
        nextIndex = Math.min(currentIndex + 1, years.length - 1);
        break;
      case "ArrowLeft":
        nextIndex = Math.max(currentIndex - 1, 0);
        break;
      case "ArrowDown":
        nextIndex = Math.min(currentIndex + 3, years.length - 1);
        break;
      case "ArrowUp":
        nextIndex = Math.max(currentIndex - 3, 0);
        break;
      case "Home":
        nextIndex = 0;
        break;
      case "End":
        nextIndex = years.length - 1;
        break;
      default:
        return;
    }
    if (nextIndex !== currentIndex) {
      const nextYear = years[nextIndex];
      if (nextYear == null) return;
      e.preventDefault();
      setActiveYear(nextYear);
      focusYearCell(nextYear);
    }
  };
  const contextValue = React__default.useMemo(() => ({
    activeYear,
    focusedYear,
    getFormattedYear,
    isYearPickerOpen,
    selectYear: handleYearSelect,
    setActiveYear,
    slots,
    years
  }), [activeYear, focusedYear, getFormattedYear, handleYearSelect, isYearPickerOpen, slots, years]);
  return /*#__PURE__*/jsx(CalendarYearPickerGridContext, {
    value: contextValue,
    children: /*#__PURE__*/jsx(dom.div, {
      ref: gridRef,
      "aria-hidden": !isYearPickerOpen,
      "aria-label": yearGridAriaLabel,
      className: composeSlotClassName(slots.yearGrid, className),
      "data-open": isYearPickerOpen || undefined,
      "data-slot": "calendar-year-picker-grid",
      role: "listbox",
      tabIndex: -1,
      onKeyDown: handleKeyDown,
      ...props,
      children: children
    })
  });
};
CalendarYearPickerGrid.displayName = "HeroUI.CalendarYearPicker.Grid";

/* -------------------------------------------------------------------------------------------------
 * CalendarYearPickerGridBody
 * -----------------------------------------------------------------------------------------------*/
const CalendarYearPickerGridBody = ({
  children
}) => {
  const {
    focusedYear,
    getFormattedYear,
    isYearPickerOpen,
    selectYear,
    years
  } = useCalendarYearPickerGridContext();
  const currentYear = new Date().getFullYear();
  return /*#__PURE__*/jsx(Fragment, {
    children: years.map(year => {
      const isSelected = year === focusedYear;
      const formattedYear = getFormattedYear(year);
      const values = {
        formattedYear,
        isCurrentYear: year === currentYear,
        isOpen: isYearPickerOpen,
        isSelected,
        selectYear: () => selectYear(year),
        year
      };
      return /*#__PURE__*/jsx(React__default.Fragment, {
        children: typeof children === "function" ? children(values) : /*#__PURE__*/jsx(CalendarYearPickerCell, {
          year: year
        })
      }, year);
    })
  });
};
CalendarYearPickerGridBody.displayName = "HeroUI.CalendarYearPicker.GridBody";

/* -------------------------------------------------------------------------------------------------
 * CalendarYearPickerCell
 * -----------------------------------------------------------------------------------------------*/
const CalendarYearPickerCell = ({
  children,
  className,
  excludeFromTabOrder,
  onFocus,
  onPress,
  year,
  ...props
}) => {
  const {
    activeYear,
    focusedYear,
    getFormattedYear,
    isYearPickerOpen,
    selectYear,
    setActiveYear,
    slots
  } = useCalendarYearPickerGridContext();
  const isSelected = year === focusedYear;
  const isActive = year === activeYear;
  const formattedYear = getFormattedYear(year);
  const values = {
    formattedYear,
    isCurrentYear: year === new Date().getFullYear(),
    isOpen: isYearPickerOpen,
    isSelected,
    selectYear: () => selectYear(year),
    year
  };
  return /*#__PURE__*/jsx(Button, {
    "aria-label": formattedYear,
    "aria-selected": isSelected,
    className: composeTwRenderProps(className, slots.yearCell()),
    "data-selected": isSelected || undefined,
    "data-slot": "calendar-year-picker-year-cell",
    "data-year": year,
    excludeFromTabOrder: excludeFromTabOrder ?? !(isYearPickerOpen && isActive),
    slot: null,
    onFocus: event => {
      onFocus?.(event);
      setActiveYear(year);
    },
    onPress: event => {
      onPress?.(event);
      selectYear(year);
    },
    ...props,
    children: typeof children === "function" ? children(values) : children || formattedYear
  });
};
CalendarYearPickerCell.displayName = "HeroUI.CalendarYearPicker.Cell";

export { CalendarYearPickerCell, CalendarYearPickerGrid, CalendarYearPickerGridBody, CalendarYearPickerTrigger, CalendarYearPickerTriggerHeading, CalendarYearPickerTriggerIndicator };
