import type { DateValue } from "@internationalized/date";
/**
 * Returns the year offset between the Gregorian calendar and the given
 * calendar system.  Used to compute sensible default min/max year bounds.
 */
export declare function getGregorianYearOffset(identifier: string): number;
/**
 * Iterates from `start` to `end` using calendar-aware arithmetic so that
 * non-Gregorian calendars (e.g. Japanese, Hebrew) produce correct year
 * boundaries.  Ported from HeroUI v2.
 */
export declare function getYearRange(start?: DateValue | null, end?: DateValue | null): DateValue[];
type DayOfWeek = "sun" | "mon" | "tue" | "wed" | "thu" | "fri" | "sat";
type WeekdayStyle = "narrow" | "short" | "long";
export declare function getDayViewWeekDayLabels(start: DateValue, locale: string, firstDayOfWeek: DayOfWeek | undefined, weekdayStyle?: WeekdayStyle, timeZone?: string): string[];
/**
 * Builds week-aligned rows for day view. The first row starts at `startOfWeek(start)`
 * (leading dates before `start` are shown but disabled by RAC). Each subsequent row
 * starts on the next week boundary.
 */
export declare function getDayViewGridRows(start: DateValue, end: DateValue, locale: string, firstDayOfWeek?: DayOfWeek): (DateValue | null)[][];
export {};
