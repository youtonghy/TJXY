export type NumberPartType = Exclude<Intl.NumberFormatPartTypes, 'minusSign' | 'plusSign'> | 'sign' | 'prefix' | 'suffix';
type IntegerPart = {
    type: NumberPartType & 'integer';
    value: number;
};
type FractionPart = {
    type: NumberPartType & 'fraction';
    value: number;
};
type DigitPart = IntegerPart | FractionPart;
type SymbolPart = {
    type: Exclude<NumberPartType, 'integer' | 'fraction'>;
    value: string;
};
export type NumberPartKey = string;
type KeyedPart = {
    key: NumberPartKey;
};
export type KeyedDigitPart = DigitPart & KeyedPart & {
    pos: number;
};
export type KeyedSymbolPart = SymbolPart & KeyedPart;
export type KeyedNumberPart = KeyedDigitPart | KeyedSymbolPart;
export type Format = Omit<Intl.NumberFormatOptions, 'notation'> & {
    notation?: Exclude<Intl.NumberFormatOptions['notation'], 'scientific' | 'engineering'>;
};
export type Value = Exclude<Parameters<typeof Intl.NumberFormat.prototype.formatToParts>[0], bigint | undefined>;
export declare function formatToData(value: Value, formatter: Intl.NumberFormat, prefix?: string, suffix?: string): {
    pre: KeyedNumberPart[];
    integer: KeyedNumberPart[];
    fraction: KeyedNumberPart[];
    post: KeyedNumberPart[];
    valueAsString: string;
    value: number;
};
export type Data = ReturnType<typeof formatToData>;
export {};
