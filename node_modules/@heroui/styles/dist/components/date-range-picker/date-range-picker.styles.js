import { tv } from 'tailwind-variants';

const dateRangePickerVariants = tv({
    slots: {
        base: "date-range-picker",
        popover: "date-range-picker__popover",
        rangeSeparator: "date-range-picker__range-separator",
        trigger: "date-range-picker__trigger",
        triggerIndicator: "date-range-picker__trigger-indicator",
    },
});

export { dateRangePickerVariants };
