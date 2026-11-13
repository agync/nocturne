// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  type DetectorClass,
  type FrameSource,
  pickReader,
  readFrame,
  type ScannerEnv,
} from "./scanner";

const frame: FrameSource = {
  width: 2,
  height: 2,
  draw: () => ({ data: new Uint8ClampedArray(16), width: 2, height: 2 }) as ImageData,
};

function detector(formats: string[] | Error, found: string[] = []): DetectorClass {
  return class {
    static async getSupportedFormats() {
      if (formats instanceof Error) throw formats;
      return formats;
    }
    async detect() {
      return found.map((rawValue) => ({ rawValue }));
    }
  } as unknown as DetectorClass;
}

describe("the scanner", () => {
  it("scanner.feature_detects_then_falls_back_to_jsqr", async () => {
    let loaded = 0;
    const loadJsqr = async () => {
      loaded += 1;
      return () => ({ data: "https://x.trycloudflare.com/r#x.trycloudflare.com" });
    };
    const native: ScannerEnv = {
      BarcodeDetector: detector(["qr_code", "ean_13"], ["from native"]),
      loadJsqr,
    };
    expect(await pickReader(native)).toBe("native");
    expect(await readFrame("native", native, {} as CanvasImageSource, frame)).toBe("from native");
    expect(loaded).toBe(0);

    for (const env of [
      { loadJsqr },
      { BarcodeDetector: detector(["ean_13"]), loadJsqr },
      { BarcodeDetector: detector(new Error("blocked")), loadJsqr },
    ] as ScannerEnv[]) {
      expect(await pickReader(env)).toBe("jsqr");
    }
    expect(await readFrame("jsqr", { loadJsqr }, {} as CanvasImageSource, frame)).toBe(
      "https://x.trycloudflare.com/r#x.trycloudflare.com",
    );
    expect(loaded).toBe(1);
  });

  it("reads nothing from an empty frame or a frame with no code", async () => {
    const loadJsqr = async () => () => null;
    expect(await readFrame("jsqr", { loadJsqr }, {} as CanvasImageSource, frame)).toBeNull();
    expect(
      await readFrame("jsqr", { loadJsqr }, {} as CanvasImageSource, {
        ...frame,
        draw: () => null,
      }),
    ).toBeNull();
    const env: ScannerEnv = { BarcodeDetector: detector(["qr_code"], []), loadJsqr };
    expect(await readFrame("native", env, {} as CanvasImageSource, frame)).toBeNull();
  });
});
