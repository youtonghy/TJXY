import { tv } from 'tailwind-variants';

const colorInputGroupVariants = tv({
    defaultVariants: {
        fullWidth: false,
        variant: "primary",
    },
    slots: {
        base: "color-input-group",
        input: "color-input-group__input",
        prefix: "color-input-group__prefix",
        suffix: "color-input-group__suffix",
    },
    variants: {
        fullWidth: {
            false: {},
            true: {
                base: "color-input-group--full-width",
            },
        },
        variant: {
            primary: {
                base: "color-input-group--primary",
            },
            secondary: {
                base: "color-input-group--secondary",
            },
        },
    },
});

export { colorInputGroupVariants };
