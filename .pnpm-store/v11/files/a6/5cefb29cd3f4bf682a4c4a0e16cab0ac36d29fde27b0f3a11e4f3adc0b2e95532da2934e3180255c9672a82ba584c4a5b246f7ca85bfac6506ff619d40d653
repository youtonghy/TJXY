import {MessageDictionary as $gt8dg$MessageDictionary, MessageFormatter as $gt8dg$MessageFormatter} from "@internationalized/message";
import {useMemo as $gt8dg$useMemo, useCallback as $gt8dg$useCallback} from "react";
import {useLocale as $gt8dg$useLocale} from "react-aria/I18nProvider";

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


const $345fc832e5b22baa$var$cache = new WeakMap();
function $345fc832e5b22baa$var$getCachedDictionary(strings) {
    let dictionary = $345fc832e5b22baa$var$cache.get(strings);
    if (!dictionary) {
        dictionary = new (0, $gt8dg$MessageDictionary)(strings);
        $345fc832e5b22baa$var$cache.set(strings, dictionary);
    }
    return dictionary;
}
function $345fc832e5b22baa$export$ec23bf898b1eed85(strings) {
    let { locale: locale } = (0, $gt8dg$useLocale)();
    let dictionary = (0, $gt8dg$useMemo)(()=>$345fc832e5b22baa$var$getCachedDictionary(strings), [
        strings
    ]);
    let formatter = (0, $gt8dg$useMemo)(()=>new (0, $gt8dg$MessageFormatter)(locale, dictionary), [
        locale,
        dictionary
    ]);
    return (0, $gt8dg$useCallback)((key, variables)=>formatter.format(key, variables), [
        formatter
    ]);
}


export {$345fc832e5b22baa$export$ec23bf898b1eed85 as useMessageFormatter};
//# sourceMappingURL=useMessageFormatter.js.map
