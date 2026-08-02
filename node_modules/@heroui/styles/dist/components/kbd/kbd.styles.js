import { tv } from 'tailwind-variants';

const kbdVariants = tv({
    defaultVariants: {},
    slots: {
        abbr: "kbd__abbr",
        base: "kbd",
        content: "kbd__content",
    },
    variants: {
        variant: {
            default: "kbd--default",
            light: "kbd--light",
        },
    },
});

export { kbdVariants };
