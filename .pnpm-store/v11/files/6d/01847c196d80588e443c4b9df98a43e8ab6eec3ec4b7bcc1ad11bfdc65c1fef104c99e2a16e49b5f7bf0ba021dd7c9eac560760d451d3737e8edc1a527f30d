import type { AvatarVariants } from "@heroui/styles";
import type { ComponentPropsWithRef } from "react";
import * as AvatarPrimitive from "@radix-ui/react-avatar";
interface AvatarRootProps extends Omit<ComponentPropsWithRef<typeof AvatarPrimitive.Root>, "color">, AvatarVariants {
}
declare const AvatarRoot: ({ children, className, color, size, variant, ...props }: AvatarRootProps) => import("react/jsx-runtime").JSX.Element;
interface AvatarImageProps extends ComponentPropsWithRef<typeof AvatarPrimitive.Image> {
}
declare const AvatarImage: ({ className, crossOrigin, loading, onError, onLoad, sizes, src, srcSet, ...props }: AvatarImageProps) => import("react/jsx-runtime").JSX.Element;
interface AvatarFallbackProps extends ComponentPropsWithRef<typeof AvatarPrimitive.Fallback> {
    color?: AvatarVariants["color"];
}
declare const AvatarFallback: ({ className, color, ...props }: AvatarFallbackProps) => import("react/jsx-runtime").JSX.Element;
export { AvatarRoot, AvatarImage, AvatarFallback };
export type { AvatarRootProps, AvatarImageProps, AvatarFallbackProps };
