var $048d76b84370f141$exports = require("./utils.cjs");
var $433949643203e332$exports = require("./Autocomplete.cjs");
var $d5d46822336ca1e1$exports = require("./Label.cjs");
var $cab7d9a238d19c33$exports = require("./Text.cjs");
var $4P2rL$reactariaprivatecollectionsHidden = require("react-aria/private/collections/Hidden");
var $4P2rL$reactariafilterDOMProps = require("react-aria/filterDOMProps");
var $4P2rL$reactariauseHover = require("react-aria/useHover");
var $4P2rL$reactariamergeProps = require("react-aria/mergeProps");
var $4P2rL$reactariamergeRefs = require("react-aria/mergeRefs");
var $4P2rL$react = require("react");
var $4P2rL$reactstatelyuseTokenFieldState = require("react-stately/useTokenFieldState");
var $4P2rL$reactariauseFocusRing = require("react-aria/useFocusRing");
var $4P2rL$reactariauseObjectRef = require("react-aria/useObjectRef");
var $4P2rL$reactariauseTokenField = require("react-aria/useTokenField");


function $parcel$interopDefault(a) {
  return a && a.__esModule ? a.default : a;
}

function $parcel$export(e, n, v, s) {
  Object.defineProperty(e, n, {get: v, set: s, enumerable: true, configurable: true});
}

$parcel$export(module.exports, "TokenFieldContext", function () { return $95baf1bfb6adc271$export$24532e5e37c56ff2; });
$parcel$export(module.exports, "TokenField", function () { return $95baf1bfb6adc271$export$e4550c392cbe69d7; });
$parcel$export(module.exports, "TokenInput", function () { return $95baf1bfb6adc271$export$4ddf413e820cfcc4; });
$parcel$export(module.exports, "Token", function () { return $95baf1bfb6adc271$export$50792b0e93539fde; });
/*
 * Copyright 2026 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing permissions and limitations under the License.
 */ 













const $95baf1bfb6adc271$export$24532e5e37c56ff2 = /*#__PURE__*/ (0, $4P2rL$react.createContext)(null);
const $95baf1bfb6adc271$var$TokenInputContext = /*#__PURE__*/ (0, $4P2rL$react.createContext)(null);
const $95baf1bfb6adc271$export$e4550c392cbe69d7 = /*#__PURE__*/ (0, $4P2rL$reactariaprivatecollectionsHidden.createHideableComponent)(function TokenField(props, ref) {
    [props, ref] = (0, $048d76b84370f141$exports.useContextProps)(props, ref, $95baf1bfb6adc271$export$24532e5e37c56ff2);
    let [labelRef, label] = (0, $048d76b84370f141$exports.useSlot)(!props['aria-label'] && !props['aria-labelledby']);
    let fieldCtx = (0, $048d76b84370f141$exports.useSlottedContext)((0, $433949643203e332$exports.FieldInputContext), props.slot);
    let { value: _autocompleteValue, onChange: onAutocompleteChange, ref: autocompleteRef, ...autocompleteProps } = fieldCtx ?? {};
    let inputRef = (0, $4P2rL$reactariauseObjectRef.useObjectRef)(autocompleteRef);
    let isDisabled = props.isDisabled || false;
    let isReadOnly = props.isReadOnly || false;
    let state = (0, $4P2rL$reactstatelyuseTokenFieldState.useTokenFieldState)({
        ...props,
        onChange: (value)=>{
            props.onChange?.(value);
            onAutocompleteChange?.(value.toString());
        }
    });
    let { tokenFieldProps: tokenFieldProps, labelProps: labelProps, descriptionProps: descriptionProps } = (0, $4P2rL$reactariauseTokenField.useTokenField)({
        ...props,
        label: // @ts-ignore - not a public prop, used to determine if slot is present
        label,
        role: props.role || autocompleteProps['role'] || 'textbox'
    }, state, inputRef);
    let renderProps = (0, $048d76b84370f141$exports.useRenderProps)({
        ...props,
        values: {
            isDisabled: isDisabled,
            isReadOnly: isReadOnly
        },
        defaultClassName: 'react-aria-TokenField'
    });
    let DOMProps = (0, $4P2rL$reactariafilterDOMProps.filterDOMProps)(props, {
        global: true
    });
    return /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement((0, $048d76b84370f141$exports.dom).div, {
        ...DOMProps,
        ...renderProps,
        ref: ref,
        slot: props.slot || undefined,
        "data-disabled": isDisabled || undefined,
        "data-readonly": isReadOnly || undefined
    }, /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement((0, $048d76b84370f141$exports.Provider), {
        values: [
            [
                (0, $d5d46822336ca1e1$exports.LabelContext),
                {
                    ...labelProps,
                    elementType: 'span',
                    ref: labelRef
                }
            ],
            [
                (0, $cab7d9a238d19c33$exports.TextContext),
                {
                    slots: {
                        description: descriptionProps
                    }
                }
            ],
            [
                $95baf1bfb6adc271$var$TokenInputContext,
                {
                    tokenFieldProps: tokenFieldProps,
                    state: state,
                    isDisabled: isDisabled,
                    isReadOnly: isReadOnly,
                    autocompleteProps: autocompleteProps,
                    ref: inputRef
                }
            ]
        ]
    }, renderProps.children));
});
const $95baf1bfb6adc271$export$4ddf413e820cfcc4 = /*#__PURE__*/ (0, $4P2rL$react.forwardRef)(function TokenInput(props, forwardedRef) {
    let { tokenFieldProps: tokenFieldProps, state: state, isDisabled: isDisabled = false, isReadOnly: isReadOnly = false, autocompleteProps: autocompleteProps, ref: contextRef } = (0, $4P2rL$react.useContext)($95baf1bfb6adc271$var$TokenInputContext);
    let ref = (0, $4P2rL$react.useMemo)(()=>(0, $4P2rL$reactariamergeRefs.mergeRefs)(contextRef, forwardedRef), [
        contextRef,
        forwardedRef
    ]);
    let { children: children, ...domProps } = props;
    let { isHovered: isHovered, hoverProps: hoverProps } = (0, $4P2rL$reactariauseHover.useHover)(domProps);
    let { isFocused: isFocused, isFocusVisible: isFocusVisible, focusProps: focusProps } = (0, $4P2rL$reactariauseFocusRing.useFocusRing)();
    let renderProps = (0, $048d76b84370f141$exports.useRenderProps)({
        ...domProps,
        defaultClassName: 'react-aria-TokenInput',
        values: {
            isHovered: isHovered,
            isFocused: isFocused,
            isFocusVisible: isFocusVisible,
            isDisabled: isDisabled,
            isReadOnly: isReadOnly
        }
    });
    let DOMProps = (0, $4P2rL$reactariafilterDOMProps.filterDOMProps)(domProps, {
        global: true
    });
    return /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement((0, $048d76b84370f141$exports.dom).div, {
        ...(0, $4P2rL$reactariamergeProps.mergeProps)(DOMProps, renderProps, focusProps, hoverProps, tokenFieldProps, autocompleteProps),
        ref: ref,
        "data-focused": isFocused || undefined,
        "data-focus-visible": isFocusVisible || undefined,
        "data-disabled": isDisabled || undefined,
        "data-readonly": isReadOnly || undefined,
        style: {
            ...renderProps.style,
            ...tokenFieldProps?.style
        }
    }, /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement($95baf1bfb6adc271$var$CompositionRenderBlocker, {
        isComposing: state.isComposing
    }, state.value.segments.map((v, i)=>{
        switch(v.type){
            case 'token':
                {
                    let token = children(v);
                    return(// Wrap tokens in zero-width spaces so the cursor is placed correctly.
                    /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement("span", {
                        key: i
                    }, '\u200b', token, '\u200b'));
                }
            case 'text':
                return v.text;
        }
    }), state.value.segments.at(-1)?.text.endsWith('\n') && /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement("br", null)));
});
const $95baf1bfb6adc271$export$50792b0e93539fde = /*#__PURE__*/ (0, $4P2rL$react.forwardRef)(function Token(props, ref) {
    let { isDisabled: isDisabled } = (0, $4P2rL$react.useContext)($95baf1bfb6adc271$var$TokenInputContext);
    let objectRef = (0, $4P2rL$reactariauseObjectRef.useObjectRef)(ref);
    let { tokenProps: tokenProps, isSelected: isSelected } = (0, $4P2rL$reactariauseTokenField.useToken)(props, {}, objectRef);
    let renderProps = (0, $048d76b84370f141$exports.useRenderProps)({
        ...props,
        defaultClassName: 'react-aria-Token',
        values: {
            isSelected: isSelected,
            isDisabled: isDisabled
        }
    });
    let DOMProps = (0, $4P2rL$reactariafilterDOMProps.filterDOMProps)(props, {
        global: true
    });
    return /*#__PURE__*/ (0, ($parcel$interopDefault($4P2rL$react))).createElement((0, $048d76b84370f141$exports.dom).span, {
        ref: objectRef,
        ...(0, $4P2rL$reactariamergeProps.mergeProps)(DOMProps, renderProps, tokenProps),
        "data-selected": isSelected || undefined,
        "data-disabled": isDisabled || undefined,
        style: {
            ...renderProps.style,
            ...tokenProps.style
        }
    }, renderProps.children);
});
// Prevents React from re-rendering during composition events.
const $95baf1bfb6adc271$var$CompositionRenderBlocker = /*#__PURE__*/ (0, $4P2rL$react.memo)(({ children: children })=>children, (prevProps, nextProps)=>nextProps.isComposing ? true : prevProps.children === nextProps.children);


//# sourceMappingURL=TokenField.cjs.map
