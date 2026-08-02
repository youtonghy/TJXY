import { tv } from 'tailwind-variants';

const toolbarVariants = tv({
    base: "toolbar",
    defaultVariants: {
        isAttached: false,
        orientation: "horizontal",
    },
    variants: {
        isAttached: {
            true: "toolbar--attached",
        },
        orientation: {
            horizontal: "toolbar--horizontal",
            vertical: "toolbar--vertical",
        },
    },
});

export { toolbarVariants };
