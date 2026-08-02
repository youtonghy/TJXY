import { tv } from 'tailwind-variants';

const dropdownVariants = tv({
    slots: {
        menu: "dropdown__menu",
        popover: "dropdown__popover",
        root: "dropdown",
        trigger: "dropdown__trigger",
    },
});

export { dropdownVariants };
