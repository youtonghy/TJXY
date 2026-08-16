import { Node } from '@tiptap/core';
import { Node as Node$1 } from '@tiptap/pm/model';

interface TaskItemOptions {
    /**
     * A callback function that is called when the checkbox is clicked while the editor is in readonly mode.
     * @param node The prosemirror node of the task item
     * @param checked The new checked state
     * @returns boolean
     */
    onReadOnlyChecked?: (node: Node$1, checked: boolean) => boolean;
    /**
     * Controls whether the task items can be nested or not.
     * @default false
     * @example true
     */
    nested: boolean;
    /**
     * HTML attributes to add to the task item element.
     * @default {}
     * @example { class: 'foo' }
     */
    HTMLAttributes: Record<string, any>;
    /**
     * The node type for taskList nodes
     * @default 'taskList'
     * @example 'myCustomTaskList'
     */
    taskListTypeName: string;
    /**
     * Accessibility options for the task item.
     * @default {}
     * @example
     * ```js
     * {
     *   checkboxLabel: (node) => `Task item: ${node.textContent || 'empty task item'}`
     * }
     */
    a11y?: {
        /**
         * The accessible label for the checkbox. Used as the aria-label on the input and as the
         * (visually hidden) text of the wrapping label element.
         * @param node The prosemirror node of the task item
         * @param checked The new checked state
         * @returns The accessible label for the checkbox
         * @example
         * ```js
         * TaskItem.configure({
         *   a11y: {
         *     checkboxLabel: node => `Task item: ${node.textContent || 'empty task item'}`,
         *   },
         * })
         * ```
         */
        checkboxLabel?: (node: Node$1, checked: boolean) => string;
    };
}
/**
 * Matches a task item to a - [ ] on input.
 */
declare const inputRegex: RegExp;
/**
 * This extension allows you to create task items.
 * @see https://www.tiptap.dev/api/nodes/task-item
 */
declare const TaskItem: Node<TaskItemOptions, any>;

export { TaskItem, type TaskItemOptions, inputRegex };
