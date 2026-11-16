// The English pages offer, in one line under the header, the same page in the reader's language
// when the browser lists one of the site's languages ahead of English. It reads only the browser's
// own list, sends nothing and sets no cookie; closing the line is remembered in local storage.
// The build fills in the words for each language.
(() => {
  const OFFER = /*@SUGGEST@*/ {};
  const KEY = "nocturne-language-offer";

  const closed = () => {
    try {
      return localStorage.getItem(KEY) === "closed";
    } catch {
      return false;
    }
  };

  // The first of the browser's languages that the site has; none when English comes first.
  const choose = () => {
    const wanted = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language];
    for (const tag of wanted) {
      const code = String(tag || "").toLowerCase().split("-")[0];
      if (code === "en") return null;
      if (Object.prototype.hasOwnProperty.call(OFFER, code)) return code;
    }
    return null;
  };

  document.addEventListener("DOMContentLoaded", () => {
    if (closed()) return;
    const code = choose();
    const header = document.querySelector("header.top");
    if (!code || !header) return;
    const words = OFFER[code];

    const line = document.createElement("p");
    line.className = "offer";
    line.lang = words.lang;
    line.append(`${words.text} `);

    const link = document.createElement("a");
    link.href = `/${code}${location.pathname}${location.hash}`;
    link.hreflang = code;
    link.textContent = words.link;

    const close = document.createElement("button");
    close.type = "button";
    close.className = "offer-close";
    close.setAttribute("aria-label", words.close);
    close.textContent = "×";
    close.addEventListener("click", () => {
      try {
        localStorage.setItem(KEY, "closed");
      } catch {
        // Storage can be blocked; the line then comes back on the next page.
      }
      line.remove();
    });

    line.append(link, close);
    header.after(line);
  });
})();
