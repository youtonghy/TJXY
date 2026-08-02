import type { ComponentProps } from "react";
import { RadioContent, RadioControl, RadioIndicator, RadioRoot } from "./radio";
export declare const Radio: {
    ({ children, className, ...props }: import("./radio").RadioRootProps): import("react/jsx-runtime").JSX.Element;
    displayName: string;
} & {
    Root: {
        ({ children, className, ...props }: import("./radio").RadioRootProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Content: {
        ({ children, className, ...props }: import("./radio").RadioContentProps): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Control: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./radio").RadioControlProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./radio").RadioControlProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
    Indicator: {
        <E extends keyof React.JSX.IntrinsicElements = "span">({ children, className, ...props }: import("./radio").RadioIndicatorProps<E> & Omit<React.JSX.IntrinsicElements[E], keyof import("./radio").RadioIndicatorProps<E>>): import("react/jsx-runtime").JSX.Element;
        displayName: string;
    };
};
export type Radio = {
    Props: ComponentProps<typeof RadioRoot>;
    RootProps: ComponentProps<typeof RadioRoot>;
    ContentProps: ComponentProps<typeof RadioContent>;
    ControlProps: ComponentProps<typeof RadioControl>;
    IndicatorProps: ComponentProps<typeof RadioIndicator>;
};
export { RadioContent, RadioControl, RadioIndicator, RadioRoot };
export type { RadioRootProps, RadioRootProps as RadioProps, RadioContentProps, RadioControlProps, RadioIndicatorProps, RadioFieldRenderProps, RadioButtonRenderProps, } from "./radio";
export { radioVariants } from "@heroui/styles";
export type { RadioVariants } from "@heroui/styles";
