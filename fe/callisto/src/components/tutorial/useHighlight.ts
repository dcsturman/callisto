import { useEffect } from "react";

const CLASS = "tutorial-target";

/**
 * Outline the elements marked `data-tutorial="<marker>"` for each marker
 * given. Panels open, fold and remount all the time, so the outline is
 * re-applied on a short timer rather than once.
 */
export function useHighlight(markers: string[] | undefined) {
  const key = (markers ?? []).join(" ");
  useEffect(() => {
    if (key === "") return undefined;
    const selector = key
      .split(" ")
      .map((marker) => `[data-tutorial="${marker}"]`)
      .join(", ");
    const apply = () => {
      const wanted = new Set(document.querySelectorAll(selector));
      document.querySelectorAll(`.${CLASS}`).forEach((el) => {
        if (!wanted.has(el)) el.classList.remove(CLASS);
      });
      wanted.forEach((el) => el.classList.add(CLASS));
    };
    apply();
    const timer = window.setInterval(apply, 400);
    return () => {
      window.clearInterval(timer);
      document.querySelectorAll(`.${CLASS}`).forEach((el) => el.classList.remove(CLASS));
    };
  }, [key]);
}
