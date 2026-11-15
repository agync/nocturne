// The install page: puts the download for this computer first. It reads only what the browser says
// about itself and sends nothing anywhere. Without it, every platform is listed in a fixed order.
(() => {
  const MESSAGES = {
    mac: "This looks like a Mac, so its download comes first. Nocturne needs a Mac with an Apple chip.",
    macArm: "This looks like a Mac with an Apple chip, so its download comes first.",
    macIntel:
      "This looks like a Mac with an Intel chip. Nocturne needs a Mac with an Apple chip (M1 or later), so it will not run on this one. About This Mac, in the Apple menu, shows which chip it has.",
    windows: "This looks like a Windows computer, so its download comes first.",
    linux: "This looks like a Linux computer, so its choices come first.",
    linuxArm: "This looks like a Linux computer with an Arm chip, so its choices come first, Arm downloads first.",
    other:
      "Nocturne is a desktop app, for Mac, Windows and Linux computers. Open this page on the computer you want to use it on, or choose one below.",
  };

  // The browser's own report, from the newer client hints where it has them.
  const detect = () => {
    const hints = navigator.userAgentData;
    const ua = navigator.userAgent || "";
    const platform = (hints && hints.platform) || navigator.platform || "";
    if ((hints && hints.mobile) || /Android|iPhone|iPad|iPod|Mobile/i.test(ua)) return "other";
    if (/CrOS|Chrome ?OS/i.test(ua + " " + platform)) return "other";
    if (/Mac/i.test(platform) || /Macintosh|Mac OS X/.test(ua)) {
      // An iPad asks for desktop pages as a Mac; only it has a touch screen.
      return navigator.maxTouchPoints > 1 ? "other" : "mac";
    }
    if (/Win/i.test(platform) || /Windows/.test(ua)) return "windows";
    if (/Linux|X11/i.test(platform) || /Linux|X11/.test(ua)) return "linux";
    return "other";
  };

  // Only Chrome and Edge tell an Arm chip from an Intel one through the client hints; Safari and
  // Firefox report a Mac as Intel either way, so their answer is not used.
  const chip = async () => {
    const hints = navigator.userAgentData;
    if (!hints || typeof hints.getHighEntropyValues !== "function") return "";
    try {
      const { architecture } = await hints.getHighEntropyValues(["architecture"]);
      return architecture === "arm" ? "arm" : architecture === "x86" ? "intel" : "";
    } catch {
      return "";
    }
  };

  const lead = (list, os) => {
    const section = list.querySelector(`[data-platform="${os}"]`);
    if (!section) return;
    list.prepend(section);
    section.classList.add("is-here");
    const tag = section.querySelector("[data-here-tag]");
    if (tag) tag.hidden = false;
    const others = list.querySelector("[data-others]");
    if (others) {
      section.after(others);
      others.hidden = false;
    }
    list.classList.add("is-sorted");
  };

  // On Linux the Intel or AMD download of each package comes first, unless the browser says the
  // chip is Arm: through the client hints, or Firefox naming it in its own report.
  const linuxArm = async () => {
    if (/aarch64|arm64/i.test(`${navigator.userAgent || ""} ${navigator.platform || ""}`)) return true;
    return (await chip()) === "arm";
  };

  const armFirst = (list) => {
    for (const pair of list.querySelectorAll("[data-arch]")) {
      const arm = pair.querySelector('[data-chip="arm"]');
      if (arm) pair.prepend(arm);
    }
  };

  const undo = (list, os) => {
    const section = list.querySelector(`[data-platform="${os}"]`);
    if (!section) return;
    section.classList.remove("is-here");
    const tag = section.querySelector("[data-here-tag]");
    if (tag) tag.hidden = true;
    const others = list.querySelector("[data-others]");
    if (others) others.hidden = true;
    list.classList.remove("is-sorted");
  };

  document.addEventListener("DOMContentLoaded", () => {
    const list = document.querySelector("[data-platforms]");
    const here = document.querySelector("[data-here]");
    if (!list || !here) return;
    const os = detect();
    here.textContent = MESSAGES[os];
    if (os === "other") return;
    lead(list, os);
    if (os === "linux") {
      linuxArm().then((arm) => {
        if (!arm) return;
        here.textContent = MESSAGES.linuxArm;
        armFirst(list);
      });
      return;
    }
    if (os !== "mac") return;
    chip().then((found) => {
      if (found === "arm") here.textContent = MESSAGES.macArm;
      if (found === "intel") {
        here.textContent = MESSAGES.macIntel;
        undo(list, "mac");
      }
    });
  });
})();
