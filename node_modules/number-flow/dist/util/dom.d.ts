type ExcludeReadonly<T> = {
    -readonly [K in keyof T as T[K] extends Readonly<any> ? never : K]: T[K];
};
export type HTMLProps<K extends keyof HTMLElementTagNameMap> = Partial<ExcludeReadonly<HTMLElementTagNameMap[K]> & {
    part: string;
}>;
export declare const createElement: <K extends keyof HTMLElementTagNameMap>(tagName: K, optionsOrChildren?: HTMLProps<K> | Node[], _children?: Node[]) => HTMLElementTagNameMap[K];
export type Justify = 'left' | 'right';
export declare const offset: (el: HTMLElement, justify: Justify) => number;
export declare const visible: (el: HTMLElement) => boolean;
export declare const define: (name: string, constructor: CustomElementConstructor) => void;
export {};
