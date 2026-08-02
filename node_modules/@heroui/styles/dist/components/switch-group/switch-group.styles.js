import { tv } from 'tailwind-variants';

const switchGroupVariants = tv({
    defaultVariants: {
        orientation: "vertical",
    },
    slots: {
        base: "switch-group",
        items: "switch-group__items",
    },
    variants: {
        orientation: {
            horizontal: {
                base: "switch-group--horizontal",
            },
            vertical: {
                base: "switch-group--vertical",
            },
        },
    },
});

export { switchGroupVariants };
