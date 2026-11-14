// The "In the app" tour. On wide screens, when motion is welcome, the Keep, Ask and page steps
// share one still frame that stays beside the text: as each step reaches the middle of the screen
// only the page inside the frame changes, and a highlighter pass shows where to look. Without
// this script, on narrow screens or with reduced motion, each step keeps its own picture.
(() => {
  const wanted = window.matchMedia(
    "(min-width: 960px) and (prefers-reduced-motion: no-preference)",
  );

  const MARK = [
    {
      opacity: 0,
      clipPath: "inset(0 100% 0 0 round 8px)",
      easing: "cubic-bezier(0.16, 1, 0.3, 1)",
    },
    { opacity: 1, clipPath: "inset(0 0% 0 0 round 8px)", offset: 0.24 },
    { opacity: 1, clipPath: "inset(0 0% 0 0 round 8px)", offset: 0.72, easing: "ease-in" },
    { opacity: 0, clipPath: "inset(0 0% 0 0 round 8px)" },
  ];

  function start(tour) {
    const steps = [...tour.querySelectorAll(":scope > .feature")];
    const frame = document.createElement("div");
    frame.className = "tour-frame";
    frame.setAttribute("aria-hidden", "true");
    const stage = document.createElement("div");
    stage.className = "tour-stage";

    const slides = steps.map((step) => {
      const slide = document.createElement("div");
      slide.className = "tour-slide";
      for (const picture of step.querySelectorAll(".feature-shot picture")) {
        const copy = picture.cloneNode(true);
        for (const source of copy.querySelectorAll("source")) source.remove();
        for (const img of copy.querySelectorAll("img")) img.alt = "";
        slide.append(copy);
      }
      return slide;
    });

    const mark = document.createElement("div");
    mark.className = "tour-mark";
    stage.append(...slides, mark);
    frame.append(stage);
    tour.append(frame);
    tour.classList.add("is-live");

    let current = 0;
    let marked = -1;
    let settle = 0;
    slides[0].classList.add("is-current");

    const show = (index) => {
      if (index !== current) {
        const before = current;
        current = index;
        slides.forEach((slide, i) => {
          slide.classList.toggle("is-current", i === index);
          slide.classList.toggle("is-before", i === before);
        });
        clearTimeout(settle);
        settle = setTimeout(() => slides[before].classList.remove("is-before"), 560);
      }
      if (index === marked) return;
      marked = index;
      const box = (steps[index].dataset.mark ?? "").split(" ").map(Number);
      for (const animation of mark.getAnimations()) animation.cancel();
      if (box.length !== 4 || box.some(Number.isNaN)) return;
      ["--mx", "--my", "--mw", "--mh"].forEach((name, i) => {
        mark.style.setProperty(name, box[i]);
      });
      mark.animate(MARK, { duration: 2600, delay: 320 });
    };

    // A step is current while it crosses the middle line of the screen.
    const watcher = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) show(steps.indexOf(entry.target));
        }
      },
      { rootMargin: "-50% 0px -50% 0px" },
    );
    for (const step of steps) watcher.observe(step);

    return () => {
      watcher.disconnect();
      clearTimeout(settle);
      frame.remove();
      tour.classList.remove("is-live");
    };
  }

  const tour = document.querySelector("[data-tour]");
  if (!tour || !("IntersectionObserver" in window)) return;
  let stop = null;
  const apply = () => {
    if (wanted.matches && !stop) stop = start(tour);
    else if (!wanted.matches && stop) {
      stop();
      stop = null;
    }
  };
  wanted.addEventListener("change", apply);
  apply();
})();
