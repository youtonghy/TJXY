import { ServerSafeHTMLElement } from './ssr';
export default class NumberFlowGroup extends ServerSafeHTMLElement {
    private _mutationObserver?;
    connectedCallback(): void;
    private _flows;
    private _addDescendant;
    private _removeDescendant;
    private _onDescendantConnected;
    private _updating;
    private _onDescendantUpdate;
    disconnectedCallback(): void;
}
declare global {
    interface HTMLElementTagNameMap {
        'number-flow-group': NumberFlowGroup;
    }
}
