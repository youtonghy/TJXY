import { tv } from 'tailwind-variants';

const popoverVariants = tv({
    slots: {
        base: "popover",
        dialog: "popover__dialog",
        heading: "popover__heading",
        trigger: "popover__trigger",
    },
});

export { popoverVariants };
