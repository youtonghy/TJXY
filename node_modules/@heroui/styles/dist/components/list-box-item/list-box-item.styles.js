import { tv } from 'tailwind-variants';

const listboxItemVariants = tv({
    defaultVariants: {
        variant: "default",
    },
    slots: {
        indicator: "list-box-item__indicator",
        item: "list-box-item",
    },
    variants: {
        variant: {
            danger: {
                item: "list-box-item--danger",
            },
            default: {
                item: "list-box-item--default",
            },
        },
    },
});

export { listboxItemVariants };
