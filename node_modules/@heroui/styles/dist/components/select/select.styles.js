import { tv } from 'tailwind-variants';

const selectVariants = tv({
    defaultVariants: {
        fullWidth: false,
        variant: "primary",
    },
    slots: {
        base: "select",
        indicator: "select__indicator",
        popover: "select__popover",
        trigger: "select__trigger",
        value: "select__value",
    },
    variants: {
        fullWidth: {
            false: {},
            true: {
                base: "select--full-width",
                trigger: "select__trigger--full-width",
            },
        },
        variant: {
            primary: {
                base: "select--primary",
            },
            secondary: {
                base: "select--secondary",
            },
        },
    },
});

export { selectVariants };
