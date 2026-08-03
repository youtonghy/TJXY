import { ApiError } from './httpClient';

export function validText(
  value: unknown,
  maxLength: number,
  allowEmpty = false,
): value is string {
  return typeof value === 'string'
    && (allowEmpty || value.trim().length > 0)
    && Array.from(value).length <= maxLength
    && !hasControlCharacters(value);
}

export function validMultilineText(
  value: unknown,
  maxLength: number,
  allowEmpty = false,
): value is string {
  return typeof value === 'string'
    && (allowEmpty || value.trim().length > 0)
    && Array.from(value).length <= maxLength
    && !Array.from(value).some((character) => {
      const codePoint = character.codePointAt(0) ?? 0;
      return (codePoint < 0x20 && character !== '\n' && character !== '\t') || codePoint === 0x7f;
    });
}

export function validUuid(value: unknown): value is string {
  return typeof value === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value);
}

export function validDate(value: unknown): value is string {
  return typeof value === 'string' && value.length <= 64 && !Number.isNaN(Date.parse(value));
}

export function hasControlCharacters(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0) ?? 0;
    return codePoint < 0x20 || codePoint === 0x7f;
  });
}

export function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0;
}

export function invalidResponse(subject: string): ApiError {
  return new ApiError(200, 'invalid-response', `The server returned an invalid ${subject}.`);
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
