import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { TextInput } from "./TextInput";

function Harness({ error }: { error?: string }) {
  const [value, setValue] = useState("");
  return (
    <TextInput
      label="Name"
      hint="As you would say it"
      error={error}
      value={value}
      onChange={setValue}
    />
  );
}

describe("TextInput", () => {
  it("has a visible label and reads the hint with the field", () => {
    render(<Harness />);
    const field = screen.getByRole("textbox", { name: "Name" });
    expect(field).toHaveAccessibleDescription("As you would say it");
    expect(field).not.toHaveAttribute("aria-invalid");
  });

  it("is controlled and passes what is typed", async () => {
    render(<Harness />);
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "Leo");
    expect(screen.getByRole("textbox")).toHaveValue("Leo");
  });

  it("marks itself invalid and reads the error after the hint", () => {
    render(<Harness error="Say who this is about." />);
    const field = screen.getByRole("textbox", { name: "Name" });
    expect(field).toBeInvalid();
    expect(field).toHaveAccessibleDescription("As you would say it Say who this is about.");
    expect(screen.getByRole("alert")).toHaveTextContent("Say who this is about.");
  });

  it("keeps its alert mounted and empty when all is well", () => {
    render(<Harness />);
    expect(screen.getByRole("alert", { hidden: true })).toBeEmptyDOMElement();
  });
});
