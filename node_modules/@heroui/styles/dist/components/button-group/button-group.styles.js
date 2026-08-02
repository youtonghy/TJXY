import { tv } from 'tailwind-variants';

const buttonGroupVariants = tv({
    defaultVariants: {
        fullWidth: false,
        orientation: "horizontal",
    },
    slots: {
        base: "button-group",
        separator: "button-group__separator",
    },
    variants: {
        fullWidth: {
            false: {},
            true: {
                base: "button-group--full-width",
            },
        },
        orientation: {
            horizontal: {
                base: "button-group--horizontal",
            },
            vertical: {
                base: "button-group--vertical",
            },
        },
    },
});

export { buttonGroupVariants };
