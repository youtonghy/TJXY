import type { ReactElement } from "react";
import React from "react";
export type DOMRenderFunction<E extends keyof React.JSX.IntrinsicElements, T> = (props: React.JSX.IntrinsicElements[E], renderProps: T) => ReactElement;
export interface DOMRenderProps<E extends keyof React.JSX.IntrinsicElements, T> {
    /**
     * Overrides the default DOM element with a custom render function.
     * This allows rendering existing components with built-in styles and behaviors
     * such as router links, animation libraries, and pre-styled components.
     *
     * Requirements:
     *
     * * You must render the expected element type (e.g. if `<button>` is expected, you cannot render an `<a>`).
     * * Only a single root DOM element can be rendered (no fragments).
     * * You must pass through props and ref to the underlying DOM element, merging with your own prop as appropriate.
     */
    render?: DOMRenderFunction<E, T>;
}
type DOMComponents = {
    [E in keyof React.JSX.IntrinsicElements]: (props: DOMRenderProps<E, any> & React.JSX.IntrinsicElements[E]) => ReactElement;
};
export declare const dom: DOMComponents;
export {};
