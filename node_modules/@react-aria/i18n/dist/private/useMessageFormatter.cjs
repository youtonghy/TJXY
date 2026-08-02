var $gCv12$internationalizedmessage = require("@internationalized/message");
var $gCv12$react = require("react");
var $gCv12$reactariaI18nProvider = require("react-aria/I18nProvider");


function $parcel$export(e, n, v, s) {
  Object.defineProperty(e, n, {get: v, set: s, enumerable: true, configurable: true});
}

$parcel$export(module.exports, "useMessageFormatter", function () { return $640f8936efd5afb8$export$ec23bf898b1eed85; });
/*
 * Copyright 2020 Adobe. All rights reserved.
 * This file is licensed to you under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License. You may obtain a copy
 * of the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software distributed under
 * the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR REPRESENTATIONS
 * OF ANY KIND, either express or implied. See the License for the specific language
 * governing permissions and limitations under the License.
 */ 


const $640f8936efd5afb8$var$cache = new WeakMap();
function $640f8936efd5afb8$var$getCachedDictionary(strings) {
    let dictionary = $640f8936efd5afb8$var$cache.get(strings);
    if (!dictionary) {
        dictionary = new (0, $gCv12$internationalizedmessage.MessageDictionary)(strings);
        $640f8936efd5afb8$var$cache.set(strings, dictionary);
    }
    return dictionary;
}
function $640f8936efd5afb8$export$ec23bf898b1eed85(strings) {
    let { locale: locale } = (0, $gCv12$reactariaI18nProvider.useLocale)();
    let dictionary = (0, $gCv12$react.useMemo)(()=>$640f8936efd5afb8$var$getCachedDictionary(strings), [
        strings
    ]);
    let formatter = (0, $gCv12$react.useMemo)(()=>new (0, $gCv12$internationalizedmessage.MessageFormatter)(locale, dictionary), [
        locale,
        dictionary
    ]);
    return (0, $gCv12$react.useCallback)((key, variables)=>formatter.format(key, variables), [
        formatter
    ]);
}


//# sourceMappingURL=useMessageFormatter.cjs.map
