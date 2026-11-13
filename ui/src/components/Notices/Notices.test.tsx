import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { copyDeck } from "../../copy/t";
import type { Notice, NoticeCode } from "../../engine/bindings";
import { Notices, noticeAction, noticeText } from "./Notices";

const CODES: Record<NoticeCode, true> = {
  downloadStalled: true,
  diskFull: true,
  tierDropped: true,
  tierUnfinished: true,
  restarts: true,
  couldntRead: true,
  micDenied: true,
  printFailed: true,
  catchingUp: true,
  repairNeeded: true,
  notReady: true,
  tidyPausedBattery: true,
  updateRolledBack: true,
  reReading: true,
};

const make = (code: NoticeCode, extra: Partial<Notice> = {}): Notice => ({
  id: `n-${code}`,
  code,
  itemId: null,
  etaSeconds: null,
  bytes: null,
  ...extra,
});

describe("Notices", () => {
  it("has one sentence and one action in the copy deck for every code that is shown", () => {
    for (const code of (Object.keys(CODES) as NoticeCode[]).filter((c) => c !== "printFailed")) {
      expect(
        copyDeck[`notice.${code}.text`] ?? copyDeck[`notice.${code}.text.unknown`],
        code,
      ).toBeTruthy();
      expect(noticeText(make(code)).length, code).toBeGreaterThan(10);
      expect(noticeAction(code).length, code).toBeGreaterThan(1);
    }
  });

  it("shows nothing for a failed print: nothing prints by itself", () => {
    render(<Notices notices={[make("printFailed")]} />);
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("never shows an error code", () => {
    for (const code of Object.keys(CODES) as NoticeCode[]) {
      expect(noticeText(make(code))).not.toMatch(/E-\d\d/);
    }
  });

  it("says how much space is needed and how long a catch-up will take", () => {
    expect(noticeText(make("diskFull", { bytes: 6_000_000_000 }))).toBe(
      "This computer needs 6 GB more free space to finish setting up.",
    );
    expect(noticeText(make("catchingUp", { etaSeconds: 40 }))).toBe(
      "The computer was off overnight. Catching up now, about 40 seconds.",
    );
    expect(noticeText(make("catchingUp", { etaSeconds: 300 }))).toContain("about 5 min");
  });

  it("shows a button only when something can be done, and runs it", async () => {
    const onRetry = vi.fn();
    const notices = [make("couldntRead", { itemId: "i1" }), make("restarts")];
    render(<Notices notices={notices} handlers={{ couldntRead: onRetry }} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.getAllByRole("button")).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(onRetry).toHaveBeenCalledWith(notices[0]);
  });

  it("offers the smaller mind with what it costs to download, and a way to say not now", async () => {
    const choose = vi.fn();
    const decline = vi.fn();
    const notice = make("tierDropped", { bytes: 3_660_000_000 });
    expect(noticeText(notice)).toBe(
      "This computer is running short of computer memory. Nocturne can switch to a smaller mind, which needs 3.7 GB to download. Nothing you have kept is lost.",
    );
    expect(noticeText(make("tierDropped"))).not.toContain("{size}");
    render(
      <Notices
        notices={[notice]}
        handlers={{ tierDropped: choose }}
        dismissals={{ tierDropped: decline }}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(decline).toHaveBeenCalledWith(notice);
    expect(choose).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Use the smaller one" }));
    expect(choose).toHaveBeenCalledWith(notice);
  });

  it("keeps both live regions mounted, even with nothing to say", () => {
    render(<Notices notices={[]} />);
    expect(screen.getByRole("alert")).toBeEmptyDOMElement();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
    expect(screen.queryByRole("listitem")).toBeNull();
  });

  it("announces what stops something working at once, and the rest politely", () => {
    const urgent: NoticeCode[] = ["diskFull", "downloadStalled", "repairNeeded"];
    const all = (Object.keys(CODES) as NoticeCode[]).map((c) => make(c));
    render(<Notices notices={all} />);
    const alert = screen.getByRole("alert");
    const status = screen.getByRole("status");
    expect(within(alert).getAllByRole("listitem")).toHaveLength(urgent.length);
    for (const code of urgent) {
      expect(alert.querySelector(`[data-code="${code}"]`), code).not.toBeNull();
      expect(status.querySelector(`[data-code="${code}"]`), code).toBeNull();
    }
    expect(status.querySelector('[data-code="restarts"]')).not.toBeNull();
    expect(status).toHaveAttribute("aria-live", "polite");
  });
});
