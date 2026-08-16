import { tv } from 'tailwind-variants';

const autocompleteVariants = tv({
    defaultVariants: {
        fullWidth: false,
        variant: "primary",
    },
    slots: {
        base: "autocomplete",
        clearButton: "autocomplete__clear-button",
        filter: "autocomplete__filter",
        indicator: "autocomplete__indicator",
        popover: "autocomplete__popover",
        trigger: "autocomplete__trigger",
        value: "autocomplete__value",
    },
    variants: {
        fullWidth: {
            false: {},
            true: {
                base: "autocomplete--full-width",
                trigger: "autocomplete__trigger--full-width",
            },
        },
        variant: {
            primary: {
                base: "autocomplete--primary",
            },
            secondary: {
                base: "autocomplete--secondary",
            },
        },
    },
});

export { autocompleteVariants };
