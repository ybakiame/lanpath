import { useEffect, useRef } from "react";
import "./connection-water.css";

export type ConnectionPhase = "idle" | "starting" | "connected" | "stopping" | "error";

/** Decorative only: CSS moves the water; no frame loop or connection/traffic inference. */
export function ConnectionWater({ phase }: { phase: ConnectionPhase }) {
  const waterRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const water = waterRef.current;
    const card = water?.parentElement;
    if (!water || !card) return;
    const ripple = water.querySelector<HTMLElement>(".connection-water-ripple");
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    let inView = false;
    let animation: Animation | undefined;
    const update = () => {
      const active = inView && !document.hidden && !motion.matches;
      water.dataset.active = String(active);
      if (!active) {
        animation?.cancel();
        animation = undefined;
      }
    };
    const observer = new IntersectionObserver(([entry]) => {
      inView = entry.isIntersecting;
      update();
    });
    observer.observe(card);
    const disturb = (event: PointerEvent) => {
      if (water.dataset.active !== "true" || !event.isPrimary || !ripple) return;
      const bounds = water.getBoundingClientRect();
      ripple.style.left = `${event.clientX - bounds.left}px`;
      ripple.style.top = `${event.clientY - bounds.top}px`;
      animation?.cancel();
      animation = ripple.animate(
        [
          { transform: "translate(-50%, -50%) scale(0.08)", opacity: 0.45 },
          { transform: "translate(-50%, -50%) scale(0.72)", opacity: 0.14, offset: 0.55 },
          { transform: "translate(-50%, -50%) scale(1)", opacity: 0 },
        ],
        { duration: 1800, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
      );
    };
    card.addEventListener("pointerdown", disturb, { passive: true });
    document.addEventListener("visibilitychange", update);
    motion.addEventListener("change", update);
    return () => {
      observer.disconnect();
      animation?.cancel();
      card.removeEventListener("pointerdown", disturb);
      document.removeEventListener("visibilitychange", update);
      motion.removeEventListener("change", update);
    };
  }, []);

  return (
    <div ref={waterRef} className="connection-water" data-phase={phase} data-active="false" aria-hidden="true">
      <div className="connection-water-surface">
        <svg viewBox="0 0 800 240" preserveAspectRatio="none" className="connection-water-layer connection-water-back">
          <path d="M0 105 Q100 50 200 105 T400 105 T600 105 T800 105 V240 H0Z" />
        </svg>
        <svg
          viewBox="0 0 800 240"
          preserveAspectRatio="none"
          className="connection-water-layer connection-water-middle"
        >
          <path d="M0 120 Q100 165 200 120 T400 120 T600 120 T800 120 V240 H0Z" />
        </svg>
        <svg viewBox="0 0 800 240" preserveAspectRatio="none" className="connection-water-layer connection-water-front">
          <path d="M0 145 Q100 110 200 145 T400 145 T600 145 T800 145 V240 H0Z" />
        </svg>
      </div>
      <span className="connection-water-ripple" />
    </div>
  );
}
