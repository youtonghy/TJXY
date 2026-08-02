import NumberFlowLite from './lite';
import { type Value, type Format } from './formatter';
export declare const styles: readonly [string, string, string];
export * from './lite';
export declare const CONNECT_EVENT = "number-flow-connect";
export declare const UPDATE_EVENT = "number-flow-update";
export declare const renderInnerHTML: (value: Value, { locales, format, numberPrefix: prefix, numberSuffix: suffix, nonce }?: {
    locales?: Intl.LocalesArgument;
    format?: Intl.NumberFormatOptions;
    numberPrefix?: string;
    numberSuffix?: string;
    nonce?: string;
}) => string;
export default class NumberFlow extends NumberFlowLite {
    /**
     * @internal for grouping
     */
    connected: boolean;
    connectedCallback(): void;
    disconnectedCallback(): void;
    format?: Format;
    locales?: Intl.LocalesArgument;
    numberPrefix?: string;
    numberSuffix?: string;
    private _formatter?;
    private _prevFormat?;
    private _prevLocales?;
    private _value?;
    get value(): Value | undefined;
    update(value?: Value): void;
}
declare global {
    interface HTMLElementTagNameMap {
        'number-flow': NumberFlow;
    }
}
