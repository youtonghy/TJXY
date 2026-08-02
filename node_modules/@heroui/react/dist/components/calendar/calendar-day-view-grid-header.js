"use client";
import React__default from 'react';
import { useLocale } from 'react-aria-components/I18nProvider';
import { getDayViewWeekDayLabels } from '../../utils/calendar.js';
import { dom } from '../../utils/dom.js';
import { jsx } from 'react/jsx-runtime';

function CalendarDayViewGridHeader({
  children,
  className,
  "data-slot": dataSlot = "calendar-grid-header",
  firstDayOfWeek,
  timeZone = "UTC",
  visibleRange,
  weekdayStyle = "short"
}) {
  const {
    locale
  } = useLocale();
  const weekDays = React__default.useMemo(() => getDayViewWeekDayLabels(visibleRange.start, locale, firstDayOfWeek, weekdayStyle, timeZone), [firstDayOfWeek, locale, timeZone, visibleRange.start, weekdayStyle]);
  return /*#__PURE__*/jsx(dom.thead, {
    "aria-hidden": true,
    className: className,
    "data-slot": dataSlot,
    children: /*#__PURE__*/jsx("tr", {
      children: weekDays.map((day, index) => /*#__PURE__*/React__default.cloneElement(children(day), {
        key: index
      }))
    })
  });
}

export { CalendarDayViewGridHeader };
