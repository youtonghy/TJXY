import { define as r } from "./lite.mjs";
import { S as a } from "./ssr-DvIINv8w.mjs";
import d, { CONNECT_EVENT as s, UPDATE_EVENT as n } from "./index.mjs";
class o extends a {
  constructor() {
    super(...arguments), this._flows = /* @__PURE__ */ new Set(), this._addDescendant = (e) => {
      e.batched = !0, this._flows.add(e);
    }, this._removeDescendant = (e) => {
      e.batched = !1, this._flows.delete(e);
    }, this._onDescendantConnected = (e) => {
      this._addDescendant(e.target);
    }, this._updating = !1, this._onDescendantUpdate = () => {
      this._updating || (this._updating = !0, this._flows.forEach((e) => {
        e.created && (e.willUpdate(), queueMicrotask(() => {
          e.connected && e.didUpdate();
        }));
      }), queueMicrotask(() => {
        this._updating = !1;
      }));
    };
  }
  connectedCallback() {
    this.querySelectorAll("number-flow").forEach((e) => {
      this._addDescendant(e);
    }), this.addEventListener(s, this._onDescendantConnected), this.addEventListener(n, this._onDescendantUpdate), this._mutationObserver ?? (this._mutationObserver = new MutationObserver((e) => {
      e.forEach((i) => {
        i.removedNodes.forEach((t) => {
          t instanceof d && this._removeDescendant(t);
        });
      });
    })), this._mutationObserver.observe(this, { childList: !0, subtree: !0 });
  }
  disconnectedCallback() {
    var e;
    this.removeEventListener(s, this._onDescendantConnected), this.removeEventListener(n, this._onDescendantUpdate), (e = this._mutationObserver) == null || e.disconnect();
  }
}
r("number-flow-group", o);
export {
  o as default
};
