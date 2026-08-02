var $74b2c5b1e7ea9589$exports = require("../live-announcer/LiveAnnouncer.cjs");
var $da02ee888921bc9e$exports = require("../utils/shadowdom/DOMFunctions.cjs");
var $49582955cc364b1c$exports = require("../utils/domHelpers.cjs");
var $d0b4a781cf26e80b$exports = require("../utils/platform.cjs");
var $89b39774f3b79dbb$exports = require("../utils/mergeProps.cjs");
var $d0df89f3abe2c2ca$exports = require("../interactions/useFocusVisible.cjs");
var $6e76e65001bbcda2$exports = require("../utils/useEvent.cjs");
var $e3486d9c44549186$exports = require("../label/useField.cjs");
var $cfe896014413cb8c$exports = require("../interactions/useFocusable.cjs");
var $6d2f10bb8b359da5$exports = require("../interactions/useKeyboard.cjs");
var $429333cab433657c$exports = require("../utils/useLayoutEffect.cjs");
var $2522e612fa919664$exports = require("../i18n/I18nProvider.cjs");
var $bmmaU$react = require("react");
var $bmmaU$reactstatelyuseTokenFieldState = require("react-stately/useTokenFieldState");


function $parcel$export(e, n, v, s) {
  Object.defineProperty(e, n, {get: v, set: s, enumerable: true, configurable: true});
}

$parcel$export(module.exports, "useTokenField", function () { return $a3a7b013e6c801ed$export$adf3c8de3c7afc8b; });
$parcel$export(module.exports, "getSelection", function () { return $a3a7b013e6c801ed$export$ca798a7e6e94638c; });
$parcel$export(module.exports, "setTokenFieldSelection", function () { return $a3a7b013e6c801ed$export$9cafaa09e78b6e4b; });
$parcel$export(module.exports, "tokenFieldPositionToDOMRange", function () { return $a3a7b013e6c801ed$export$30729a8cd0694a7c; });
/*
 * Copyright 2026 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing
 */ 













const $a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE = 'application/vnd.react-aria.tokens+json';
function $a3a7b013e6c801ed$export$adf3c8de3c7afc8b(props, state, ref) {
    let { role: role = 'textbox', allowsNewlines: multiline = false, isReadOnly: isReadOnly = false, isDisabled: isDisabled = false, 'aria-details': ariaDetails } = props;
    let { value: value } = state;
    let { locale: locale } = (0, $2522e612fa919664$exports.useLocale)();
    let graphemeSegmenter = (0, $bmmaU$react.useMemo)(()=>new Intl.Segmenter(locale, {
            granularity: 'grapheme'
        }), [
        locale
    ]);
    let wordSegmenter = (0, $bmmaU$react.useMemo)(()=>new Intl.Segmenter(locale, {
            granularity: 'word'
        }), [
        locale
    ]);
    let dropPosition = (0, $bmmaU$react.useRef)(null);
    let transferredData = (0, $bmmaU$react.useRef)(null);
    let nextValue = (0, $bmmaU$react.useRef)(null);
    let apply = (fn)=>{
        state.setValue((value)=>{
            let newValue = fn(value);
            nextValue.current = newValue;
            return newValue;
        });
    };
    // Composition events are not cancelable. The browser will mutate the DOM, making it out of sync with React.
    // To account for this, we prevent React from re-rendering during composition, and track DOM mutations performed
    // by the browser. When composition ends, we revert the DOM to its original state, and re-render with React.
    // Mutating the DOM in any way during composition breaks the IME, causing composition to end unexpectedly.
    // During composition, we still emit updates via onChange to ensure that things like autocomplete work,
    // but we don't actually re-render to the DOM unless the value changes from what we expect (e.g. inserting a completion).
    let mutationTracker = $a3a7b013e6c801ed$var$useMutationTracker(ref);
    let startComposition = (0, $bmmaU$react.useCallback)(()=>{
        mutationTracker.start();
        state.setComposing(true);
    }, [
        state,
        mutationTracker
    ]);
    let stopComposition = (0, $bmmaU$react.useCallback)(()=>{
        mutationTracker.stop();
        state.setComposing(false);
    }, [
        state,
        mutationTracker
    ]);
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'compositionstart', ()=>{
        startComposition();
        let range = window.getSelection()?.getRangeAt(0);
        if (range) {
            let [start, end] = $a3a7b013e6c801ed$var$rangeToPositions(ref.current, range);
            // Normalize the range to ensure it is not inside a token, otherwise the browser
            // will attempt to insert the composed text into the token instead of replacing it.
            let r = $a3a7b013e6c801ed$var$createDOMRange(ref.current, start, end);
            if (r.startContainer !== range.startContainer || r.startOffset !== range.startOffset) range.setStart(r.startContainer, r.startOffset);
            if (r.endContainer !== range.endContainer || r.endOffset !== range.endOffset) range.setEnd(r.endContainer, r.endOffset);
        }
    });
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'compositionend', stopComposition);
    // If a prop update occurs during composition that doesn't match the expected value,
    // end composition and re-render the controlled value.
    (0, $429333cab433657c$exports.useLayoutEffect)(()=>{
        if (state.isComposing && value !== nextValue.current) stopComposition();
        nextValue.current = value;
    });
    let caretPosition = (0, $bmmaU$react.useRef)(null);
    (0, $429333cab433657c$exports.useLayoutEffect)(()=>{
        if (ref.current && value.caretPosition && !state.isComposing && value.caretPosition !== caretPosition.current) {
            // Only move the caret when the field is already focused.
            if (ref.current === (0, $da02ee888921bc9e$exports.getActiveElement)((0, $49582955cc364b1c$exports.getOwnerDocument)(ref.current))) $a3a7b013e6c801ed$var$setCursor(ref.current, value.caretPosition);
            caretPosition.current = value.caretPosition;
        }
    });
    // Handle text editing commands and prevent browser default behavior.
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'beforeinput', (e)=>{
        // Android sometimes doesn't fire a compositionend event before a regular input event.
        if (state.isComposing && !e.isComposing) stopComposition();
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0) return;
        let range = selection.getRangeAt(0);
        let [start, end] = $a3a7b013e6c801ed$var$rangeToPositions(ref.current, range);
        // https://www.w3.org/TR/input-events-2/#interface-InputEvent-Attributes
        switch(e.inputType){
            case 'insertText':
            case 'insertReplacementText':
            case 'insertCompositionText':
            case 'insertFromComposition':
            case 'insertFromPaste':
            case 'insertFromYank':
            case 'insertFromDrop':
                {
                    let data = [
                        {
                            type: 'text',
                            text: e.data ?? ''
                        }
                    ];
                    if (transferredData.current) {
                        data = transferredData.current;
                        transferredData.current = null;
                    } else if (e.dataTransfer) {
                        let parsed = e.dataTransfer.types.includes($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE) ? $a3a7b013e6c801ed$var$parseSegments(e.dataTransfer.getData($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE)) : null;
                        if (parsed) data = parsed;
                        else if (e.dataTransfer.types.includes('text/plain')) data[0].text = e.dataTransfer.getData('text/plain');
                    }
                    if (e.inputType === 'insertFromDrop' && dropPosition.current) {
                        start = end = dropPosition.current;
                        dropPosition.current = null;
                    }
                    if (!multiline) for (let segment of data)segment.text = segment.text.replace(/[\r\n]+/g, ' ');
                    apply((tokens)=>tokens.replaceRangeWithSegments(start, end, data, // Don't coalesce paste/drop events with other edits.
                        e.inputType === 'insertText' || e.inputType === 'insertCompositionText' || e.inputType === 'insertFromComposition'));
                    break;
                }
            case 'insertParagraph':
                if (props.onSubmit) {
                    props.onSubmit();
                    break;
                }
            case 'insertLineBreak':
                if (multiline) apply((tokens)=>tokens.replaceRange(start, end, '\n'));
                break;
            case 'deleteContentBackward':
            case 'deleteContentForward':
            case 'deleteWordBackward':
            case 'deleteWordForward':
            case 'deleteHardLineForward':
            case 'deleteHardLineBackward':
            case 'deleteSoftLineForward':
            case 'deleteSoftLineBackward':
            case 'deleteContent':
            case 'deleteByCut':
            case 'deleteCompositionText':
                if (!range.collapsed) {
                    apply((tokens)=>tokens.replaceRange(start, end, ''));
                    break;
                }
                switch(e.inputType){
                    case 'deleteContentBackward':
                        apply((tokens)=>tokens.delete(start, graphemeSegmenter, (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Backward));
                        break;
                    case 'deleteContentForward':
                        apply((tokens)=>tokens.delete(start, graphemeSegmenter, (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Forward));
                        break;
                    case 'deleteWordBackward':
                        apply((tokens)=>tokens.delete(start, wordSegmenter, (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Backward));
                        break;
                    case 'deleteWordForward':
                        apply((tokens)=>tokens.delete(start, wordSegmenter, (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Forward));
                        break;
                    case 'deleteHardLineForward':
                    case 'deleteSoftLineForward':
                        apply((tokens)=>tokens.deleteLine(start, (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Forward));
                        break;
                    case 'deleteHardLineBackward':
                    case 'deleteSoftLineBackward':
                        apply((tokens)=>tokens.deleteLine(start, (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Backward));
                        break;
                }
                break;
            case 'deleteByDrag':
                apply((tokens)=>{
                    let endOffset = start.index === end.index ? end.offset : tokens.segments[start.index].text.length;
                    let change = tokens.replaceRange(start, end, '');
                    if (dropPosition.current && dropPosition.current.index === start.index && dropPosition.current.offset >= start.offset) dropPosition.current.offset -= endOffset - start.offset;
                    return change;
                });
                break;
        }
        e.preventDefault();
    });
    let writeClipboardData = (e)=>{
        if ('clipboardData' in e) e.preventDefault();
        let selection = $a3a7b013e6c801ed$export$ca798a7e6e94638c(ref.current);
        if (!selection) return;
        let [start, end] = selection;
        let slice = value.slice(start, end);
        let dataTransfer = 'clipboardData' in e ? e.clipboardData : e.dataTransfer;
        dataTransfer?.setData($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE, JSON.stringify(slice.segments));
        dataTransfer?.setData('text/plain', slice.toString());
        if (e.type === 'cut') apply((tokens)=>tokens.replaceRange(start, end, '', false));
    };
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'copy', writeClipboardData);
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'cut', writeClipboardData);
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'dragstart', writeClipboardData);
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'paste', (e)=>{
        // Safari doesn't pass the custom clipboard data type to beforeinput dataTransfer so we handle it here.
        if (e.clipboardData && e.clipboardData.types.includes($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE)) transferredData.current = $a3a7b013e6c801ed$var$parseSegments(e.clipboardData.getData($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE));
    });
    // Store the cursor position on drop so we know where to insert when the insertFromDrop event occurs.
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'drop', (e)=>{
        if (typeof document.caretPositionFromPoint === 'function') {
            let pos = document.caretPositionFromPoint(e.clientX, e.clientY);
            if (pos) dropPosition.current = $a3a7b013e6c801ed$var$getPosition(ref.current, pos.offsetNode, pos.offset);
        } else if (typeof document.caretRangeFromPoint === 'function') {
            let range = document.caretRangeFromPoint(e.clientX, e.clientY);
            if (range) dropPosition.current = $a3a7b013e6c801ed$var$getPosition(ref.current, range.startContainer, range.startOffset);
        }
        if (e.dataTransfer && e.dataTransfer.types.includes($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE)) transferredData.current = $a3a7b013e6c801ed$var$parseSegments(e.dataTransfer.getData($a3a7b013e6c801ed$var$CLIPBOARD_MIME_TYPE));
    });
    $a3a7b013e6c801ed$var$useSelectionChange(ref, ()=>{
        if (state.isComposing) return;
        value.endCoalescing();
        // When the cursor moves next to a token, announce it.
        // Otherwise the screen reader will only announce the first/last character.
        if (window.getSelection()?.isCollapsed) {
            let [start, end] = $a3a7b013e6c801ed$export$ca798a7e6e94638c(ref.current);
            if (start.offset === 0) {
                let segment = value.segments[start.index];
                if (segment?.type !== 'token') segment = value.segments[start.index - 1];
                if (segment?.type === 'token') (0, $74b2c5b1e7ea9589$exports.announce)(segment.text, 'assertive');
                // Update the caret position in the value.
                state.setValue((value)=>value.withCaretPosition(end));
            }
        }
    });
    // Override the default triple click behavior to ensure that tokens get selected.
    // Some browsers only select the text between tokens instead of the entire line.
    (0, $6e76e65001bbcda2$exports.useEvent)(ref, 'mousedown', (e)=>{
        if (e.detail === 3) {
            let selection = $a3a7b013e6c801ed$export$ca798a7e6e94638c(ref.current);
            if (!selection) return;
            let start = value.findLineBoundary(selection[0], (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Backward);
            let end = value.findLineBoundary(selection[1], (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Forward);
            if (start && end) {
                e.preventDefault();
                $a3a7b013e6c801ed$export$9cafaa09e78b6e4b(ref.current, start, end, true);
            }
        }
    });
    let moveSelection = (direction, granularity, extend = false)=>{
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0 || !selection.focusNode || !selection.anchorNode) return false;
        // Pressing an arrow with a non-empty selection collapses it to the corresponding edge.
        // The browser handles this natively.
        if (!extend && !selection.isCollapsed) return false;
        // Move the caret using the browser's native caret movement (Selection.modify) so that
        // bidirectional text is handled correctly. Repeat until the position actually changes
        // to account for the zero width spaces around tokens.
        let pos = $a3a7b013e6c801ed$var$getPosition(ref.current, selection.focusNode, selection.focusOffset);
        while(true){
            let { focusNode: focusNode, focusOffset: focusOffset } = selection;
            selection.modify(extend ? 'extend' : 'move', direction, granularity);
            if (selection.focusNode === focusNode && selection.focusOffset === focusOffset) return false;
            let newPos = $a3a7b013e6c801ed$var$getPosition(ref.current, selection.focusNode, selection.focusOffset);
            if (!$a3a7b013e6c801ed$var$isSamePosition(pos, newPos)) return true;
        }
    };
    // macOS supports additional keyboard shortcuts for text editing.
    // We need to handle these manually so they behave consistently with tokens.
    // https://support.apple.com/en-us/102650#text
    let macShortcuts = (0, $d0b4a781cf26e80b$exports.isMac)() ? {
        'Control+a': ()=>{
            return shortcuts.Home();
        },
        'Control+e': ()=>{
            return shortcuts.End();
        },
        'Control+f': ()=>{
            return shortcuts.ArrowRight();
        },
        'Control+b': ()=>{
            return shortcuts.ArrowLeft();
        }
    } : {};
    let mod = (0, $d0b4a781cf26e80b$exports.isMac)() ? 'Meta' : 'Control';
    let wordModKey = (0, $d0b4a781cf26e80b$exports.isMac)() ? 'Alt' : 'Control';
    let shortcuts = {
        ...macShortcuts,
        [`${mod}+z`]: ()=>{
            // If composing, the browser handles undo natively.
            if (state.isComposing) return false;
            apply((state)=>state.undo());
        },
        [(0, $d0b4a781cf26e80b$exports.isMac)() ? 'Shift+Meta+z' : 'Control+y']: ()=>{
            if (state.isComposing) return false;
            apply((state)=>state.redo());
        },
        ArrowLeft: ()=>{
            return moveSelection('left', 'character');
        },
        [`${wordModKey}+ArrowLeft`]: ()=>{
            return moveSelection('left', 'word');
        },
        'Shift+ArrowLeft': ()=>{
            return moveSelection('left', 'character', true);
        },
        [`Shift+${wordModKey}+ArrowLeft`]: ()=>{
            return moveSelection('left', 'word', true);
        },
        ArrowRight: ()=>{
            return moveSelection('right', 'character');
        },
        [`${wordModKey}+ArrowRight`]: ()=>{
            return moveSelection('right', 'word');
        },
        'Shift+ArrowRight': ()=>{
            return moveSelection('right', 'character', true);
        },
        [`Shift+${wordModKey}+ArrowRight`]: ()=>{
            return moveSelection('right', 'word', true);
        },
        Home: ()=>{
            // Browsers do not behave consistently when there are tokens.
            let selection = $a3a7b013e6c801ed$export$ca798a7e6e94638c(ref.current);
            if (!selection) return false;
            let boundary = value.findLineBoundary(selection[0], (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Backward);
            if (boundary) {
                $a3a7b013e6c801ed$var$setCursor(ref.current, boundary, true);
                return true;
            }
            return false;
        },
        End: ()=>{
            let selection = $a3a7b013e6c801ed$export$ca798a7e6e94638c(ref.current);
            if (!selection) return false;
            let boundary = value.findLineBoundary(selection[1], (0, $bmmaU$reactstatelyuseTokenFieldState.TokenFieldValue).Direction.Forward);
            if (boundary) {
                $a3a7b013e6c801ed$var$setCursor(ref.current, boundary, true);
                return true;
            }
            return false;
        }
    };
    // TODO: user provided onKeyDown currently relies on user provided preventDefault to stop submit
    // maybe can have them specify a format like shortcuts and merge into above?
    let { keyboardProps: keyboardProps } = (0, $6d2f10bb8b359da5$exports.useKeyboard)({
        isDisabled: isDisabled || isReadOnly,
        onKeyDown: props.onKeyDown,
        onKeyUp: props.onKeyUp,
        shortcuts: shortcuts,
        allowRepeats: true
    });
    let { focusableProps: focusableProps } = (0, $cfe896014413cb8c$exports.useFocusable)(props, ref);
    let { labelProps: labelProps, fieldProps: fieldProps, descriptionProps: descriptionProps } = (0, $e3486d9c44549186$exports.useField)({
        ...props,
        labelElementType: 'span'
    });
    return {
        labelProps: {
            ...labelProps,
            onClick: ()=>{
                if (!props.isDisabled) {
                    ref.current?.focus();
                    // Show the focus ring so the user knows where focus went
                    (0, $d0df89f3abe2c2ca$exports.setInteractionModality)('keyboard');
                }
            }
        },
        descriptionProps: descriptionProps,
        tokenFieldProps: (0, $89b39774f3b79dbb$exports.mergeProps)(focusableProps, keyboardProps, fieldProps, {
            onPaste: props.onPaste,
            onCopy: props.onCopy,
            onCut: props.onCut,
            contentEditable: !isDisabled && !isReadOnly,
            suppressContentEditableWarning: true,
            role: role,
            'aria-multiline': multiline,
            'aria-details': ariaDetails,
            'aria-readonly': isReadOnly,
            'aria-disabled': isDisabled,
            style: {
                whiteSpace: 'pre-wrap'
            }
        })
    };
}
function $a3a7b013e6c801ed$var$indexOfNode(node) {
    let index = 0;
    let n = node;
    while(n = n.previousSibling)index++;
    return index;
}
function $a3a7b013e6c801ed$export$ca798a7e6e94638c(container) {
    let selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return null;
    let range = selection.getRangeAt(0);
    return $a3a7b013e6c801ed$var$rangeToPositions(container, range);
}
function $a3a7b013e6c801ed$var$rangeToPositions(container, range) {
    let start = $a3a7b013e6c801ed$var$getPosition(container, range.startContainer, range.startOffset);
    let end = $a3a7b013e6c801ed$var$getPosition(container, range.endContainer, range.endOffset);
    return [
        start,
        end
    ];
}
function $a3a7b013e6c801ed$var$getPosition(container, node, offset) {
    if (node === container) return {
        index: offset,
        offset: 0
    };
    let originalNode = node;
    while(node.parentNode !== container)node = node.parentNode;
    let index = $a3a7b013e6c801ed$var$indexOfNode(node);
    if (node.nodeType === Node.ELEMENT_NODE) {
        let tokenNode = node.childNodes[1];
        let atEnd;
        let endOffset = 0;
        if (originalNode === tokenNode) // Cursor is inside the token.
        atEnd = offset > 0;
        else if (originalNode === node) // Cursor is inside the wrapper element.
        atEnd = offset > 1;
        else {
            // Cursor is on one of the zero width spaces.
            atEnd = originalNode !== tokenNode.previousSibling;
            // If the offset is greater than 1, the browser is trying to insert text into
            // the zero width space node. This will actually end up in the next text node.
            endOffset = atEnd && offset > 1 ? offset - 1 : 0;
        }
        offset = atEnd ? tokenNode?.textContent?.length ?? 0 : 0;
        // Several positions are equivalent due to the zero width spaces around tokens.
        // Normalize offset to the end of the preceding text node, or the beginning of the following node.
        if (offset === 0 && node.previousSibling?.nodeType === Node.TEXT_NODE) {
            index--;
            offset = node.previousSibling?.textContent?.length ?? 0;
        } else if (atEnd) {
            index++;
            offset = endOffset;
        }
    }
    return {
        index: index,
        offset: offset
    };
}
let $a3a7b013e6c801ed$var$isProgrammaticSelectionChange = Symbol('isProgrammaticSelectionChange');
function $a3a7b013e6c801ed$var$setCursor(root, pos, fireEvent = false) {
    $a3a7b013e6c801ed$export$9cafaa09e78b6e4b(root, pos, pos, fireEvent);
}
function $a3a7b013e6c801ed$export$9cafaa09e78b6e4b(root, start, end, fireEvent = false) {
    let selection = window.getSelection();
    if (selection) {
        let range = $a3a7b013e6c801ed$var$createDOMRange(root, start, end);
        root[$a3a7b013e6c801ed$var$isProgrammaticSelectionChange] = !fireEvent;
        selection.removeAllRanges();
        selection.addRange(range);
    }
}
function $a3a7b013e6c801ed$export$30729a8cd0694a7c(root, pos) {
    return $a3a7b013e6c801ed$var$createDOMRange(root, pos, pos);
}
function $a3a7b013e6c801ed$var$createDOMRange(root, start, end) {
    let range = document.createRange();
    let startChild = root.childNodes[start.index];
    if (!startChild) range.setStart(root, Math.min(root.childNodes.length, start.index));
    else if (startChild.nodeType === Node.ELEMENT_NODE) {
        // Place the cursor outside the token wrapper element.
        if (start.offset > 0) range.setStartAfter(startChild);
        else range.setStartBefore(startChild);
    } else range.setStart(startChild, start.offset);
    let endChild = root.childNodes[end.index];
    if (!endChild) range.setEnd(root, Math.min(root.childNodes.length, end.index));
    else if (endChild.nodeType === Node.ELEMENT_NODE) {
        if (end.offset > 0) range.setEndAfter(endChild);
        else range.setEndBefore(endChild);
    } else range.setEnd(endChild, end.offset);
    return range;
}
function $a3a7b013e6c801ed$var$isSamePosition(a, b) {
    return a.index === b.index && a.offset === b.offset;
}
// Parse and validate segments from clipboard/drag data. Returns null if the data is not valid
// JSON or does not match the expected shape, so malformed or untrusted data is ignored rather
// than throwing or being inserted into the field.
function $a3a7b013e6c801ed$var$parseSegments(json) {
    try {
        let data = JSON.parse(json);
        if (Array.isArray(data) && data.length > 0 && data.every($a3a7b013e6c801ed$var$isValidSegment)) return data;
    } catch  {
    // Ignore invalid clipboard data.
    }
    return null;
}
function $a3a7b013e6c801ed$var$isValidSegment(segment) {
    return typeof segment === 'object' && segment != null && (segment.type === 'text' || segment.type === 'token') && typeof segment.text === 'string';
}
function $a3a7b013e6c801ed$var$useSelectionChange(ref, handler) {
    (0, $6e76e65001bbcda2$exports.useEvent)((0, $bmmaU$react.useRef)(typeof document !== 'undefined' ? document : null), 'selectionchange', ()=>{
        if (ref.current && ref.current[$a3a7b013e6c801ed$var$isProgrammaticSelectionChange]) {
            ref.current[$a3a7b013e6c801ed$var$isProgrammaticSelectionChange] = false;
            return;
        }
        let selection = window.getSelection();
        if (!selection || selection.rangeCount === 0 || !ref.current) return;
        let range = selection.getRangeAt(0);
        if (range.intersectsNode(ref.current)) handler();
    });
}
function $a3a7b013e6c801ed$var$useMutationTracker(ref) {
    let mutationTracker = (0, $bmmaU$react.useRef)(null);
    // Disconnect the mutation observer if the field unmounts mid-composition.
    (0, $429333cab433657c$exports.useLayoutEffect)(()=>()=>{
            mutationTracker.current?.();
            mutationTracker.current = null;
        }, []);
    return (0, $bmmaU$react.useMemo)(()=>({
            start () {
                // Android sometimes fires two compositionstart events in a row, without a compositionend.
                // In that case, reuse the existing tracker.
                mutationTracker.current ||= $a3a7b013e6c801ed$var$trackMutations(ref.current);
            },
            stop () {
                mutationTracker.current?.();
                mutationTracker.current = null;
            }
        }), // eslint-disable-next-line react-hooks/exhaustive-deps - conflicts with compiler
    []);
}
// Tracks mutations to the DOM until the returned function is called,
// at which point the mutations are reverted.
function $a3a7b013e6c801ed$var$trackMutations(element) {
    let mutations = [];
    let observer = new MutationObserver((records)=>{
        mutations.push(...records);
    });
    observer.observe(element, {
        childList: true,
        subtree: true,
        characterData: true,
        characterDataOldValue: true
    });
    return ()=>{
        mutations.push(...observer.takeRecords());
        observer.disconnect();
        for (let record of mutations.reverse())switch(record.type){
            case 'childList':
                for (let node of record.removedNodes)record.target.insertBefore(node, record.nextSibling);
                for (let node of record.addedNodes)record.target.removeChild(node);
                break;
            case 'characterData':
                record.target.nodeValue = record.oldValue;
                break;
        }
    };
}


//# sourceMappingURL=useTokenField.cjs.map
