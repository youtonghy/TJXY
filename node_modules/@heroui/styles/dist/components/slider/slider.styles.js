import { tv } from 'tailwind-variants';

const sliderVariants = tv({
    slots: {
        base: "slider",
        fill: "slider__fill",
        marks: "slider__marks",
        output: "slider__output",
        thumb: "slider__thumb",
        track: "slider__track",
    },
});

export { sliderVariants };
