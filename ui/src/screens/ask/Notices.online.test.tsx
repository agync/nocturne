import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Notice, NoticeCode } from "../../engine/bindings";
import { createMockEngine } from "../../engine/mock/engine";
import { renderApp, setViewport } from "../../test/render";

beforeEach(() => setViewport(1280));
afterEach(() => {
  window.location.hash = "";
});

const notice = (code: NoticeCode): Notice => ({
  id: `n-${code}`,
  code,
  itemId: null,
  etaSeconds: null,
  bytes: null,
});

const open = async (code: NoticeCode) => {
  renderApp({ hash: "#/today", engine: createMockEngine({ notices: [notice(code)] }) });
  return screen.findByRole("list", { name: "Things to know" });
};

describe("online answers notices", () => {
  it.each([
    [
      "onlineKeyRefused",
      "Online answers is paused: the key wasn’t accepted. Answers are written on this computer meanwhile.",
      "Check the key",
    ],
    [
      "onlineNoCredit",
      "Online answers is paused: the account has no credit left. Answers are written on this computer meanwhile.",
      "Check again",
    ],
    [
      "onlineLimit",
      "This month’s online questions are used up. Answers are written on this computer until the 1st.",
      "See the limit",
    ],
  ] as const)(
    "notice.online_%s: says what happened, with one button that opens Online answers",
    async (code, text, button) => {
      const list = await open(code);
      expect(list).toHaveTextContent(text);
      expect(
        screen.getAllByRole("button", { name: /.+/ }).filter((b) => b.textContent === button),
      ).toHaveLength(1);
      await userEvent.click(screen.getByRole("button", { name: button }));
      await waitFor(() => expect(window.location.hash).toBe("#/settings/online"));
    },
  );

  it("notice.online_none_for_no_internet_or_slow: those pass without a notice, only the mark says", async () => {
    renderApp({ hash: "#/today", engine: createMockEngine() });
    await screen.findByRole("heading", { level: 1 });
    expect(screen.queryByRole("list", { name: "Things to know" })).toBeNull();
  });
});
