import { type HTMLProps, type Justify } from './util/dom';
import { type KeyedDigitPart, type KeyedNumberPart, type KeyedSymbolPart, type Data } from './formatter';
import { ServerSafeHTMLElement } from './ssr';
import type { Plugin } from './plugins';
export { define } from './util/dom';
export { prefersReducedMotion } from './styles';
export { renderInnerHTML } from './ssr';
export * from './plugins';
export * from './formatter';
export declare const canAnimate: boolean;
export type Trend = number | ((oldValue: number, value: number) => number);
export type DigitOptions = {
    max?: number;
};
export type Digits = Record<number, DigitOptions>;
export interface Props {
    transformTiming: EffectTiming;
    spinTiming: EffectTiming | undefined;
    opacityTiming: EffectTiming;
    animated: boolean;
    respectMotionPreference: boolean;
    trend: Trend;
    plugins?: Plugin[];
    digits: Digits | undefined;
}
export default interface NumberFlowLite extends Props {
}
/**
 * @internal Used for framework wrappers
 */
export default class NumberFlowLite extends ServerSafeHTMLElement implements Props {
    /**
     * Use `private _private` properties instead of `#private` to avoid # polyfill and
     * reduce bundle size. Also, use `readonly` properties instead of getters to save on bundle
     * size, even though you have to do gross stuff like `(this as Mutable<...>)` until TS
     * supports e.g. https://github.com/microsoft/TypeScript/issues/37487
     */
    static defaultProps: Props;
    constructor();
    private _animated;
    get animated(): boolean;
    set animated(val: boolean);
    readonly created: boolean;
    private _pre?;
    private _num?;
    private _post?;
    readonly computedTrend?: number;
    readonly computedAnimated: boolean;
    private _internals?;
    private _data?;
    /**
     * @internal
     */
    batched: boolean;
    /**
     * @internal
     */
    set data(data: Data | undefined);
    private _preUpdated;
    /**
     * @internal
     */
    willUpdate(): void;
    private _abortAnimationsFinish?;
    /**
     * @internal
     */
    didUpdate(): void;
}
type SectionProps = {
    justify: Justify;
} & HTMLProps<'span'>;
declare abstract class Section {
    readonly flow: NumberFlowLite;
    readonly el: HTMLSpanElement;
    readonly justify: Justify;
    protected children: Map<string, Char<KeyedNumberPart>>;
    constructor(flow: NumberFlowLite, parts: KeyedNumberPart[], { justify, className, ...props }: SectionProps, children?: (chars: Node[]) => Node[]);
    protected addChar(part: KeyedNumberPart, { startDigitsAtZero, ...props }?: {
        startDigitsAtZero?: boolean;
    } & Pick<AnimatePresenceProps, 'animateIn'>): Digit | Sym;
    private onCharRemove;
    protected unpop(char: Char): void;
    protected pop(chars: Map<any, Char>): void;
    protected addNewAndUpdateExisting(parts: KeyedNumberPart[]): void;
    private _prevOffset?;
    willUpdate(): void;
    didUpdate(): void;
}
type OnRemove = () => void;
interface AnimatePresenceProps {
    onRemove?: OnRemove;
    animateIn?: boolean;
}
declare class AnimatePresence {
    readonly flow: NumberFlowLite;
    readonly el: HTMLElement;
    private _present;
    private _onRemove?;
    constructor(flow: NumberFlowLite, el: HTMLElement, { onRemove, animateIn }?: AnimatePresenceProps);
    get present(): boolean;
    private _remove;
    set present(val: boolean);
}
interface CharProps extends AnimatePresenceProps {
}
declare abstract class Char<P extends KeyedNumberPart = KeyedNumberPart> extends AnimatePresence {
    readonly section: Section;
    protected value: P['value'];
    readonly el: HTMLSpanElement;
    constructor(section: Section, value: P['value'], el: HTMLSpanElement, props?: AnimatePresenceProps);
    abstract willUpdate(parentRect: DOMRect): void;
    abstract update(value: P['value']): void;
    abstract didUpdate(parentRect: DOMRect): void;
}
export declare class Digit extends Char<KeyedDigitPart> {
    readonly pos: number;
    private _numbers;
    readonly length: number;
    constructor(section: Section, type: KeyedDigitPart['type'], value: KeyedDigitPart['value'], pos: number, props?: CharProps);
    private _prevValue?;
    private _prevCenter?;
    willUpdate(parentRect: DOMRect): void;
    update(value: KeyedDigitPart['value']): void;
    didUpdate(parentRect: DOMRect): void;
    getDelta(): number;
    private _onAnimationsFinish;
}
declare class Sym extends Char<KeyedSymbolPart> {
    private type;
    constructor(section: Section, type: KeyedSymbolPart['type'], value: KeyedSymbolPart['value'], props?: CharProps);
    private _children;
    private _prevOffset?;
    willUpdate(parentRect: DOMRect): void;
    private _onChildRemove;
    update(value: KeyedSymbolPart['value']): void;
    didUpdate(parentRect: DOMRect): void;
}
