import { describe, expect, it } from "vitest";
import { fileNameOf, folderOf } from "./paths";

describe("the parts of a chosen backup path", () => {
  it("finds the name and folder in either separator", () => {
    expect(fileNameOf("/backups/Nocturne backup 2026-11-15.nocturne")).toBe(
      "Nocturne backup 2026-11-15.nocturne",
    );
    expect(folderOf("/backups/Nocturne backup 2026-11-15.nocturne")).toBe("/backups");
    expect(fileNameOf("C:\\Backups\\today.nocturne")).toBe("today.nocturne");
    expect(folderOf("C:\\Backups\\today.nocturne")).toBe("C:\\Backups");
  });

  it("keeps a bare name and a root folder sensible", () => {
    expect(fileNameOf("today.nocturne")).toBe("today.nocturne");
    expect(folderOf("today.nocturne")).toBe("");
    expect(folderOf("/today.nocturne")).toBe("/");
  });
});
