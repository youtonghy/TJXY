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
 */ var $bb130d0e51bd3001$var$Direction = /*#__PURE__*/ function(Direction) {
    Direction[Direction["Forward"] = 1] = "Forward";
    Direction[Direction["Backward"] = -1] = "Backward";
    return Direction;
}($bb130d0e51bd3001$var$Direction || {});
class $bb130d0e51bd3001$export$f3decd0c2cd2fe2e {
    createFieldValue(segments) {
        const Constructor = this.constructor;
        return new Constructor(segments);
    }
    /** Create a new list with the caret position set to the given position. */ withCaretPosition(caretPosition) {
        if (this.caretPosition.index === caretPosition.index && this.caretPosition.offset === caretPosition.offset) return this;
        let result = this.createFieldValue(this.segments);
        result.caretPosition = caretPosition;
        result.previous = this.previous;
        result.next = this.next;
        result.isCoalescing = this.isCoalescing;
        return result;
    }
    splitSegment(segment, offset) {
        if (!segment) {
            let empty = this.createTextSegment('');
            return [
                offset > 0 ? empty : null,
                offset > 0 ? null : empty
            ];
        }
        if (segment.type === 'token') return [
            offset > 0 ? {
                ...segment
            } : null,
            offset > 0 ? null : {
                ...segment
            }
        ];
        return [
            offset > 0 ? {
                type: 'text',
                text: segment.text.slice(0, offset)
            } : null,
            offset < segment.text.length ? this.createTextSegment(segment.text.slice(offset)) : null
        ];
    }
    createTextSegment(text) {
        return {
            type: 'text',
            text: text
        };
    }
    tokenize(text) {
        return [
            this.createTextSegment(text)
        ];
    }
    clampPosition(position) {
        if (this.segments.length > 0 && position.index >= this.segments.length) return {
            index: this.segments.length - 1,
            offset: this.segments[this.segments.length - 1].text.length
        };
        if (position.index < 0) return {
            index: 0,
            offset: 0
        };
        return position;
    }
    /** Replace the text between two positions with new text. */ replaceRange(start, end, text, coalesce = true) {
        return this.replaceRangeWithSegments(start, end, text.length > 0 ? [
            this.createTextSegment(text)
        ] : [], coalesce);
    }
    /** Replace the text between two positions with new segments. */ replaceRangeWithSegments(start, end, insert, coalesce = true) {
        start = this.clampPosition(start);
        end = this.clampPosition(end);
        let startSegment = this.segments[start.index];
        let endSegment = this.segments[end.index];
        let [startSplit] = this.splitSegment(startSegment, start.offset);
        let [, endSplit] = this.splitSegment(endSegment, end.offset);
        let newSegments = this.segments.slice(0, start.index);
        if (startSplit) $bb130d0e51bd3001$var$appendSegments(newSegments, [
            startSplit
        ]);
        if (insert.length) $bb130d0e51bd3001$var$appendSegments(newSegments, insert, (text)=>this.tokenize(text));
        let lastSegment = newSegments[newSegments.length - 1];
        let lastIsText = lastSegment && lastSegment.type === 'text';
        let caret = {
            index: lastIsText ? newSegments.length - 1 : newSegments.length,
            offset: lastIsText ? lastSegment.text.length : 0
        };
        if (endSplit) $bb130d0e51bd3001$var$appendSegments(newSegments, [
            endSplit
        ]);
        $bb130d0e51bd3001$var$appendSegments(newSegments, this.segments.slice(end.index + 1));
        let segments = this.createFieldValue(newSegments);
        segments.caretPosition = caret;
        segments.isCoalescing = coalesce;
        if (this.isCoalescing && coalesce && this.previous) {
            segments.previous = this.previous;
            segments.previous.next = segments;
        } else {
            segments.previous = this;
            this.caretPosition = end;
            this.next = segments;
        }
        return segments;
    }
    /** Find the boundary before or after a position using an Intl.Segmenter. */ findBoundaryWithSegmenter(position, segmenter, direction) {
        position = this.clampPosition(position);
        for(let i = position.index; i >= 0 && i < this.segments.length; i += direction){
            let segment = this.segments[i];
            switch(segment.type){
                case 'token':
                    if (i !== position.index || (direction === -1 ? position.offset > 0 : position.offset === 0)) {
                        let index = i + direction;
                        return {
                            index: index >= 0 ? index : 0,
                            offset: direction === -1 && index >= 0 ? this.segments[index].text.length : 0
                        };
                    }
                    continue;
                case 'text':
                    {
                        let offset = direction === -1 ? segment.text.length : 0;
                        if (i === position.index) offset = position.offset;
                        if (direction === -1) offset--;
                        if (offset < 0 || offset >= segment.text.length) continue;
                        let part = segmenter.segment(segment.text).containing(offset);
                        while(part && part.isWordLike === false){
                            offset += direction;
                            part = segmenter.segment(segment.text).containing(offset);
                        }
                        if (part) return {
                            index: i,
                            offset: direction === -1 ? part.index : part.index + part.segment.length
                        };
                        continue;
                    }
            }
        }
        return null;
    }
    /** Find a line boundary before or after a position. */ findLineBoundary(position, direction) {
        let res = this.findText(position, direction, '\n');
        if (res) return res;
        return direction === -1 ? {
            index: 0,
            offset: 0
        } : {
            index: this.segments.length - 1,
            offset: this.segments[this.segments.length - 1].text.length
        };
    }
    /** Find a string or regular expression match before or after a position. */ findText(position, direction, search) {
        if (this.segments.length === 0) return null;
        for(let i = position.index; i >= 0 && i < this.segments.length; i += direction){
            let segment = this.segments[i];
            if (segment.type !== 'text') continue;
            let offset = $bb130d0e51bd3001$var$findInText(segment.text, search, direction, i === position.index ? position.offset : undefined);
            if (offset >= 0) return {
                index: i,
                offset: offset
            };
        }
        return null;
    }
    /** Delete text at a position using a segmenter. */ delete(position, segmenter, direction, coalesce = true) {
        let boundary = this.findBoundaryWithSegmenter(position, segmenter, direction);
        if (boundary) return this.replaceRange(direction === -1 ? boundary : position, direction === -1 ? position : boundary, '', coalesce);
        this.caretPosition = position;
        return this;
    }
    /** Delete text to the next or previous line break. */ deleteLine(position, direction, coalesce = true) {
        if (this.segments.length === 0) return this;
        let boundary = this.findLineBoundary(position, direction);
        if (boundary) return this.replaceRange(direction === -1 ? boundary : position, direction === -1 ? position : boundary, '', coalesce);
        return this;
    }
    /** Create a new list containing a subset of the segments. */ slice(start, end) {
        start = this.clampPosition(start);
        end = this.clampPosition(end);
        if (start.index === end.index && start.offset === end.offset) return this.createFieldValue([]);
        if (start.index === end.index) {
            let segment = this.segments[start.index];
            if (segment.type === 'text') return this.createFieldValue([
                {
                    type: 'text',
                    text: segment.text.slice(start.offset, end.offset)
                }
            ]);
            return this.createFieldValue([
                segment
            ]);
        }
        let startSegment = this.segments[start.index];
        let endSegment = this.segments[end.index];
        let [, startSplit] = this.splitSegment(startSegment, start.offset);
        let [endSplit] = this.splitSegment(endSegment, end.offset);
        let result = [];
        if (startSplit) result.push(startSplit);
        result.push(...this.segments.slice(start.index + 1, end.index));
        if (endSplit) result.push(endSplit);
        return this.createFieldValue(result);
    }
    /** Convert the list to a string. */ toString() {
        return this.segments.map((seg)=>seg.text).join('');
    }
    /** Returns the previous list in the undo history. */ undo() {
        var _this_previous;
        return (_this_previous = this.previous) !== null && _this_previous !== void 0 ? _this_previous : this;
    }
    /** Returns the next list in the redo history. */ redo() {
        var _this_next;
        return (_this_next = this.next) !== null && _this_next !== void 0 ? _this_next : this;
    }
    /** End coalescing undo/redo history. */ endCoalescing() {
        this.isCoalescing = false;
    }
    /** Create a new list with the given segments. */ constructor(tokens, options){
        /** The caret position. */ this.caretPosition = {
            index: 0,
            offset: 0
        };
        // Linked list representing the undo/redo history.
        this.previous = null;
        this.next = null;
        this.isCoalescing = true;
        this.segments = tokens;
        var _options_caretPosition;
        this.caretPosition = (_options_caretPosition = options === null || options === void 0 ? void 0 : options.caretPosition) !== null && _options_caretPosition !== void 0 ? _options_caretPosition : {
            index: 0,
            offset: 0
        };
    }
}
$bb130d0e51bd3001$export$f3decd0c2cd2fe2e.Direction = $bb130d0e51bd3001$var$Direction;
function $bb130d0e51bd3001$var$findInText(text, search, direction, fromOffset) {
    var _matches_at;
    if (typeof search === 'string') {
        if (direction === -1) return text.lastIndexOf(search, fromOffset !== undefined ? fromOffset - 1 : text.length - 1);
        return text.indexOf(search, fromOffset !== null && fromOffset !== void 0 ? fromOffset : 0);
    }
    if (direction === 1) {
        let start = fromOffset !== null && fromOffset !== void 0 ? fromOffset : 0;
        let index = text.slice(start).search(search);
        return index >= 0 ? start + index : -1;
    }
    let limit = fromOffset !== undefined ? fromOffset : text.length;
    if (limit < 0) return -1;
    let re = search.flags.includes('g') ? search : new RegExp(search.source, search.flags + 'g');
    let matches = Array.from(text.slice(0, limit).matchAll(re));
    var _matches_at_index;
    return (_matches_at_index = (_matches_at = matches.at(-1)) === null || _matches_at === void 0 ? void 0 : _matches_at.index) !== null && _matches_at_index !== void 0 ? _matches_at_index : -1;
}
function $bb130d0e51bd3001$var$appendSegments(segments, insert, tokenize) {
    for (let segment of insert){
        if (segment.type === 'text' && segment.text.length === 0) continue;
        let last = segments[segments.length - 1];
        if (last && last.type === 'text' && segment.type === 'text') {
            if (tokenize) {
                let tokenized = tokenize(last.text + segment.text);
                segments.splice(segments.length - 1, 1, ...tokenized);
            } else segments[segments.length - 1] = {
                type: 'text',
                text: last.text + segment.text
            };
        } else if (tokenize && segment.type === 'text') {
            let tokenized = tokenize(segment.text);
            segments.push(...tokenized);
        } else segments.push(segment);
    }
    return segments;
}


export {$bb130d0e51bd3001$export$f3decd0c2cd2fe2e as TokenFieldValue};
//# sourceMappingURL=TokenFieldValue.js.map
