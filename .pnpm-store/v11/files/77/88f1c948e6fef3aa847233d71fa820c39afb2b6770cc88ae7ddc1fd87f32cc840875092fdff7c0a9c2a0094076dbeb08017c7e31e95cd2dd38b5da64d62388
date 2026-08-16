import { startOfYear, startOfWeek, isSameDay } from '@internationalized/date';

/**
 * Returns the year offset between the Gregorian calendar and the given
 * calendar system.  Used to compute sensible default min/max year bounds.
 */
function getGregorianYearOffset(identifier) {
  switch (identifier) {
    case "buddhist":
      return 543;
    case "ethiopic":
    case "ethioaa":
      return -8;
    case "coptic":
      return -284;
    case "hebrew":
      return 3760;
    case "indian":
      return -78;
    case "islamic-civil":
    case "islamic-tbla":
    case "islamic-umalqura":
      return -579;
    case "persian":
      return -600;
    case "roc":
    case "japanese":
    case "gregory":
    default:
      return 0;
  }
}

/**
 * Iterates from `start` to `end` using calendar-aware arithmetic so that
 * non-Gregorian calendars (e.g. Japanese, Hebrew) produce correct year
 * boundaries.  Ported from HeroUI v2.
 */
function getYearRange(start, end) {
  const years = [];
  if (!start || !end) return years;
  let current = startOfYear(start);
  while (current.compare(end) <= 0) {
    years.push(current);
    current = startOfYear(current.add({
      years: 1
    }));
  }
  return years;
}
function getDayViewWeekDayLabels(start, locale, firstDayOfWeek, weekdayStyle = "short", timeZone = "UTC") {
  const formatter = new Intl.DateTimeFormat(locale, {
    weekday: weekdayStyle,
    timeZone
  });
  const weekStart = startOfWeek(start, locale, firstDayOfWeek);
  const labels = [];
  let date = weekStart;
  for (let index = 0; index < 7; index++) {
    labels.push(formatter.format(date.toDate(timeZone)));
    const next = date.add({
      days: 1
    });
    if (isSameDay(date, next)) {
      break;
    }
    date = next;
  }
  while (labels.length < 7) {
    labels.push("");
  }
  return labels;
}
function buildDayViewWeekRow(rowStart, end) {
  const row = [];
  let date = rowStart;
  for (let index = 0; index < 7; index++) {
    row.push(date.compare(end) > 0 ? null : date);
    const next = date.add({
      days: 1
    });
    if (isSameDay(date, next)) {
      while (row.length < 7) {
        row.push(null);
      }
      return row;
    }
    date = next;
  }
  return row;
}

/**
 * Builds week-aligned rows for day view. The first row starts at `startOfWeek(start)`
 * (leading dates before `start` are shown but disabled by RAC). Each subsequent row
 * starts on the next week boundary.
 */
function getDayViewGridRows(start, end, locale, firstDayOfWeek) {
  const rows = [];
  let rowStart = startOfWeek(start, locale, firstDayOfWeek);
  rows.push(buildDayViewWeekRow(rowStart, end));
  rowStart = rowStart.add({
    weeks: 1
  });
  while (rowStart.compare(end) <= 0) {
    rows.push(buildDayViewWeekRow(rowStart, end));
    const nextWeek = rowStart.add({
      weeks: 1
    });
    if (isSameDay(rowStart, nextWeek)) {
      break;
    }
    rowStart = nextWeek;
  }
  return rows;
}

export { getDayViewGridRows, getDayViewWeekDayLabels, getGregorianYearOffset, getYearRange };
