import { useRef } from "react";
import { gsap } from "gsap";
import { useGSAP } from "@gsap/react";
gsap.registerPlugin(useGSAP);
export function useEntrance<T extends HTMLElement>(dependency: unknown = null) {
  const ref = useRef<T>(null);
  useGSAP(
    () => {
      if (typeof window.matchMedia !== "function") return;
      const media = gsap.matchMedia();
      media.add("(prefers-reduced-motion: no-preference)", () => {
        if (ref.current)
          gsap.from(ref.current, {
            autoAlpha: 0,
            y: 6,
            duration: 0.18,
            ease: "power2.out",
            clearProps: "opacity,visibility,transform",
          });
      });
      return () => media.revert();
    },
    { scope: ref, dependencies: [dependency], revertOnUpdate: true },
  );
  return ref;
}
