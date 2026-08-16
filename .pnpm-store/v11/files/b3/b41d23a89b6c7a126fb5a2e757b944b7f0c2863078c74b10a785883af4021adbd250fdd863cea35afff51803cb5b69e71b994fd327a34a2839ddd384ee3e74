import { AriaTokenFieldProps } from 'react-aria/useTokenField';
import { ClassNameOrFunction, ContextValue, RenderProps, SlotProps, StyleRenderProps } from './utils';
import { GlobalDOMAttributes } from '@react-types/shared';
import { HoverProps } from 'react-aria/useHover';
import React from 'react';
import { TokenFieldValue, TokenSegment } from 'react-stately/useTokenFieldState';
export interface TokenFieldRenderProps {
    /**
     * Whether the token field is disabled.
     *
     * @selector [data-disabled]
     */
    isDisabled: boolean;
    /**
     * Whether the token field is read only.
     *
     * @selector [data-readonly]
     */
    isReadOnly: boolean;
}
export interface TokenFieldProps<T extends TokenFieldValue = TokenFieldValue> extends AriaTokenFieldProps<T>, RenderProps<TokenFieldRenderProps>, SlotProps, GlobalDOMAttributes<HTMLDivElement> {
    /**
     * The CSS [className](https://developer.mozilla.org/en-US/docs/Web/API/Element/className) for the
     * element. A function may be provided to compute the class based on component state.
     *
     * @default 'react-aria-TokenField'
     */
    className?: ClassNameOrFunction<TokenFieldRenderProps>;
}
export interface TokenInputRenderProps {
    /**
     * Whether the token input is currently hovered with a mouse.
     *
     * @selector [data-hovered]
     */
    isHovered: boolean;
    /**
     * Whether the token input is focused, either via a mouse or keyboard.
     *
     * @selector [data-focused]
     */
    isFocused: boolean;
    /**
     * Whether the token input is keyboard focused.
     *
     * @selector [data-focus-visible]
     */
    isFocusVisible: boolean;
    /**
     * Whether the token input is disabled.
     *
     * @selector [data-disabled]
     */
    isDisabled: boolean;
    /**
     * Whether the token input is read only.
     *
     * @selector [data-readonly]
     */
    isReadOnly: boolean;
}
export interface TokenInputProps<T extends TokenFieldValue = TokenFieldValue> extends HoverProps, StyleRenderProps<TokenInputRenderProps>, SlotProps, GlobalDOMAttributes<HTMLDivElement> {
    /**
     * A function that renders a token for each segment in the token field.
     */
    children: (segment: TokenSegment<T extends TokenFieldValue<infer V> ? V : never>) => React.ReactElement;
    /**
     * The CSS [className](https://developer.mozilla.org/en-US/docs/Web/API/Element/className) for the
     * element. A function may be provided to compute the class based on component state.
     *
     * @default 'react-aria-TokenInput'
     */
    className?: ClassNameOrFunction<TokenInputRenderProps>;
}
export declare const TokenFieldContext: React.Context<ContextValue<TokenFieldProps<TokenFieldValue<any>>, HTMLDivElement>>;
/**
 * A token field allows users to enter text with inline tokens. Use it to build AI prompt fields,
 * tag inputs, structured search fields, mention inputs, and multi-select comboboxes.
 */
export declare const TokenField: <T extends TokenFieldValue<any> = TokenFieldValue<any>>(props: TokenFieldProps<T> & React.RefAttributes<HTMLDivElement>) => React.ReactElement<unknown, string | React.JSXElementConstructor<any>> | null;
/**
 * A token input represents the editable area within a token field.
 */
export declare const TokenInput: <T extends TokenFieldValue<any> = TokenFieldValue<any>>(props: TokenInputProps<T> & React.RefAttributes<HTMLDivElement>) => React.ReactElement<unknown, string | React.JSXElementConstructor<any>> | null;
export interface TokenRenderProps {
    /**
     * Whether the token is selected.
     *
     * @selector [data-selected]
     */
    isSelected: boolean;
    /**
     * Whether the token is disabled.
     *
     * @selector [data-disabled]
     */
    isDisabled: boolean;
}
export interface TokenProps extends RenderProps<TokenRenderProps, 'span'>, GlobalDOMAttributes<HTMLSpanElement> {
    /**
     * The CSS [className](https://developer.mozilla.org/en-US/docs/Web/API/Element/className) for the
     * element. A function may be provided to compute the class based on component state.
     *
     * @default 'react-aria-Token'
     */
    className?: ClassNameOrFunction<TokenRenderProps>;
}
/**
 * A token represents an inline segment within a token field.
 */
export declare const Token: React.ForwardRefExoticComponent<TokenProps & React.RefAttributes<HTMLSpanElement>>;
