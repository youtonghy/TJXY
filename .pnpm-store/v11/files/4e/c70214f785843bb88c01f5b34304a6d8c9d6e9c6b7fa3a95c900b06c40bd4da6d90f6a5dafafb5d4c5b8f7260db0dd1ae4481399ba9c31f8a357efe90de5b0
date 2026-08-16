"use client";
import React__default from 'react';
import { useLocale } from 'react-aria-components/I18nProvider';
import { getDayViewGridRows } from '../../utils/calendar.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

function CalendarDayViewGridBody({
  children,
  className,
  "data-slot": dataSlot = "calendar-grid-body",
  firstDayOfWeek,
  visibleRange
}) {
  const {
    locale
  } = useLocale();
  const rows = React__default.useMemo(() => getDayViewGridRows(visibleRange.start, visibleRange.end, locale, firstDayOfWeek), [firstDayOfWeek, locale, visibleRange.end, visibleRange.start]);
  return /*#__PURE__*/jsx(dom.tbody, {
    className: className,
    "data-slot": dataSlot,
    children: rows.map((row, weekIndex) => /*#__PURE__*/jsx("tr", {
      children: row.map((date, dayIndex) => date ? (/*#__PURE__*/React__default.cloneElement(children(date), {
        key: dayIndex
      })) : /*#__PURE__*/jsx("td", {}, dayIndex))
    }, weekIndex))
  });
}

export { CalendarDayViewGridBody };
