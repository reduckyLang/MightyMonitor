"use strict";
// A three-chapter art film: one reveal, one camera move and one timeline.
// All images are local. No permissions, telemetry or rendering dependency.
(() => {
  const stage = document.querySelector(".artwork-stage");
  const image = stage.querySelector(".workspace-art");
  const toggle = document.querySelector(".film-toggle");
  const progress = document.querySelector(".film-progress");
  const buttons = [...document.querySelectorAll(".mode-switch button")];
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  const scenes = {
    mirror: [
      "mirror",
      "Phone and laptop displaying the same green landscape. Concept artwork.",
      "Mirror a screen across your devices. Alpha software.",
    ],
    control: [
      "control",
      "A cursor reaching a phone screen. Concept artwork.",
      "Click, type and touch from another device. Control requires permission.",
    ],
    extend: [
      "extend",
      "One continuous workspace across two displays. Concept artwork.",
      "Real extra desktop space. Linux Wayland alpha; Windows integration in progress.",
    ],
  };
  let mode = "mirror",
    paused = reduced.matches,
    visible = true,
    elapsed = 0,
    last = performance.now();
  let camera, reveal;
  const duration = 6500;
  const cached = Object.values(scenes).map(([name]) => {
    const i = new Image();
    i.src = `assets/artwork/${name}.webp`;
    return i.decode().catch(() => {});
  });
  function sync() {
    const stop = paused || reduced.matches || !visible || document.hidden;
    if (camera) stop ? camera.pause() : camera.play();
    if (reveal) stop ? reveal.pause() : reveal.play();
    toggle.textContent = paused ? "Play preview" : "Pause preview";
    toggle.setAttribute("aria-pressed", String(paused));
    toggle.hidden = reduced.matches;
  }
  function select(next, animate = true) {
    mode = next;
    elapsed = 0;
    const [name, alt, description] = scenes[mode];
    const old = image.cloneNode();
    old.removeAttribute("fetchpriority");
    old.alt = "";
    old.setAttribute("aria-hidden", "true");
    old.className = "film-outgoing";
    stage.querySelector(".film-outgoing")?.remove();
    if (animate && !reduced.matches) stage.append(old);
    image.src = `assets/artwork/${name}.webp`;
    image.alt = alt;
    stage.dataset.mode = mode;
    buttons.forEach((b) =>
      b.setAttribute("aria-pressed", String(b.dataset.mode === mode)),
    );
    document.getElementById("concept-detail").textContent = description;
    camera?.cancel();
    reveal?.cancel();
    if (!reduced.matches) {
      camera = image.animate(
        [
          { transform: "scale(1.025) translateX(-.6%)" },
          { transform: "scale(1.085) translateX(.6%)" },
        ],
        { duration, fill: "forwards", easing: "linear" },
      );
      if (animate) {
        reveal = old.animate(
          [{ clipPath: "inset(0 0 0 0)" }, { clipPath: "inset(0 0 0 100%)" }],
          {
            duration: 1100,
            easing: "cubic-bezier(.76,0,.24,1)",
            fill: "forwards",
          },
        );
        reveal.onfinish = () => old.remove();
      }
    }
    sync();
  }
  toggle.hidden = false;
  toggle.addEventListener("click", () => {
    paused = !paused;
    sync();
  });
  window.addEventListener("mm-preview-mode", ({ detail }) => {
    if (scenes[detail]) select(detail);
  });
  reduced.addEventListener("change", () => {
    paused = reduced.matches;
    select(mode, false);
  });
  document.addEventListener("visibilitychange", sync);
  new IntersectionObserver((entries) => {
    visible = entries[0].isIntersecting;
    sync();
  }).observe(stage);
  setInterval(() => {
    const now = performance.now(),
      delta = Math.min(now - last, 250);
    last = now;
    if (paused || reduced.matches || !visible || document.hidden) return;
    elapsed += delta;
    progress.style.transform = `scaleX(${Math.min(elapsed / duration, 1)})`;
    if (elapsed >= duration) {
      const keys = Object.keys(scenes);
      select(keys[(keys.indexOf(mode) + 1) % keys.length]);
    }
  }, 100);
  Promise.all(cached).then(() => select("mirror", false));
})();
