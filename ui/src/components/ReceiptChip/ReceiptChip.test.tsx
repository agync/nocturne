import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Receipt } from "../../engine/bindings";
import { ReceiptChip } from "./ReceiptChip";

const base: Receipt = {
  itemId: "i1",
  kind: "audio",
  source: "recording",
  label: "Dentist moved",
  keptAt: "2025-11-10T09:00:00Z",
  durationMs: 10_000,
  span: null,
  unconfirmed: false,
  fromDevice: null,
};

describe("ReceiptChip", () => {
  it("shows the kind, the length and the day, with the voice glyph", () => {
    const { container } = render(<ReceiptChip receipt={base} onOpen={() => undefined} />);
    expect(screen.getByText("Voice note · 0:10 · Mon 10 Nov")).toBeInTheDocument();
    expect(container.querySelector('[data-kind="audio"]')).not.toBeNull();
  });

  it("names an appointment's calendar, then its phone", () => {
    const appointment: Receipt = {
      ...base,
      kind: "calendar",
      source: "phone",
      durationMs: null,
      fromDevice: "Sam’s phone",
      calendar: "Family",
    };
    render(<ReceiptChip receipt={appointment} onOpen={() => undefined} />);
    expect(screen.getByText("Calendar · Family · Sam’s phone")).toBeInTheDocument();
  });

  it("is named by the words on it, then says it is a source", () => {
    render(<ReceiptChip receipt={base} onOpen={() => undefined} />);
    expect(
      screen.getByRole("button", { name: "Voice note · 0:10 · Mon 10 Nov. Source." }),
    ).toBeInTheDocument();
  });

  it("never has an accessible name that leaves out its visible text", () => {
    const receipts: Receipt[] = [
      base,
      {
        ...base,
        kind: "web",
        source: "pastedWeb",
        label: "Nut",
        durationMs: null,
        unconfirmed: true,
      },
      { ...base, kind: "image", label: "Letter", durationMs: null },
      {
        ...base,
        kind: "document",
        label: "Brief.pdf",
        durationMs: null,
        span: { type: "page", page: 3 },
      },
      { ...base, kind: "text", source: "paste", durationMs: null },
    ];
    for (const receipt of receipts) {
      for (const onOpen of [() => undefined, undefined]) {
        const { container, unmount } = render(<ReceiptChip receipt={receipt} onOpen={onOpen} />);
        const chip = container.firstElementChild as HTMLElement;
        const visible = (chip.textContent ?? "").trim();
        expect(chip.getAttribute("aria-label"), visible).toContain(visible);
        expect(chip.getAttribute("aria-label")).not.toMatch(/Press Enter/);
        unmount();
      }
    }
  });

  it("opens the original on click and on Enter", async () => {
    const onOpen = vi.fn();
    render(<ReceiptChip receipt={base} onOpen={onOpen} />);
    await userEvent.tab();
    await userEvent.keyboard("{Enter}");
    expect(onOpen).toHaveBeenCalledWith(base);
  });

  it("uses a dashed border and says so for something unconfirmed", () => {
    const web: Receipt = {
      ...base,
      kind: "web",
      source: "pastedWeb",
      label: "Nut-free ideas",
      durationMs: null,
      unconfirmed: true,
    };
    render(<ReceiptChip receipt={web} onOpen={() => undefined} />);
    const chip = screen.getByRole("button");
    expect(chip).toHaveAttribute("data-unconfirmed", "true");
    expect(chip.className).toContain("unconfirmed");
    expect(chip).toHaveTextContent("Web page you pasted · Mon 10 Nov · unconfirmed");
    expect(chip).toHaveAccessibleName(/unconfirmed\. Source\.$/);
  });

  it("writes a photo by its label and a document by its page", () => {
    const { rerender } = render(
      <ReceiptChip receipt={{ ...base, kind: "image", label: "Letter", durationMs: null }} />,
    );
    expect(screen.getByText("Letter · Mon 10 Nov")).toBeInTheDocument();
    rerender(
      <ReceiptChip
        receipt={{
          ...base,
          kind: "document",
          label: "Brief.pdf",
          durationMs: null,
          span: { type: "page", page: 3 },
        }}
      />,
    );
    expect(screen.getByText("Brief.pdf · p.3")).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAccessibleName("Brief.pdf · p.3. Source.");
  });

  it("tells a pasted message from a typed note", () => {
    const { rerender } = render(
      <ReceiptChip receipt={{ ...base, kind: "text", source: "paste", durationMs: null }} />,
    );
    expect(screen.getByText("Message you pasted · Mon 10 Nov")).toBeInTheDocument();
    rerender(
      <ReceiptChip receipt={{ ...base, kind: "text", source: "quickCapture", durationMs: null }} />,
    );
    expect(screen.getByText("Note · Mon 10 Nov")).toBeInTheDocument();
  });

  it("is plain text, not a control, when nothing can open it", () => {
    render(<ReceiptChip receipt={base} />);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("marks a playing voice chip", () => {
    render(<ReceiptChip receipt={base} playing onOpen={() => undefined} />);
    expect(screen.getByRole("button")).toHaveAttribute("data-playing", "true");
  });
});
