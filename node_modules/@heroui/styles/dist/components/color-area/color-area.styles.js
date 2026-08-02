import { tv } from 'tailwind-variants';

const colorAreaVariants = tv({
    defaultVariants: {
        showDots: false,
    },
    slots: {
        base: "color-area",
        thumb: "color-area__thumb",
    },
    variants: {
        showDots: {
            false: {},
            true: {
                base: "color-area--show-dots",
            },
        },
    },
});

export { colorAreaVariants };
