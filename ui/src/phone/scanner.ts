// Reading a code with the camera. `BarcodeDetector` is used when feature detection finds it and
// it reads QR codes; otherwise the `jsqr` fallback reads frames drawn to a canvas, loaded only
// then. The browser is never identified by its user agent. No frame leaves the page or is kept.

export interface DetectedCode {
  rawValue: string;
}

export interface DetectorLike {
  detect(source: CanvasImageSource): Promise<DetectedCode[]>;
}

export interface DetectorClass {
  new (options: { formats: string[] }): DetectorLike;
  getSupportedFormats(): Promise<string[]>;
}

export type Jsqr = (
  data: Uint8ClampedArray,
  width: number,
  height: number,
  options?: { inversionAttempts?: "dontInvert" },
) => { data: string } | null;

export interface ScannerEnv {
  BarcodeDetector?: DetectorClass;
  loadJsqr(): Promise<Jsqr>;
}

export function defaultScannerEnv(): ScannerEnv {
  return {
    BarcodeDetector: (globalThis as { BarcodeDetector?: DetectorClass }).BarcodeDetector,
    loadJsqr: async () => (await import("jsqr")).default as Jsqr,
  };
}

export type Reader = "native" | "jsqr";

/** Native when the class exists and lists `qr_code`; the fallback otherwise, or on any error. */
export async function pickReader(env: ScannerEnv): Promise<Reader> {
  const Detector = env.BarcodeDetector;
  if (typeof Detector !== "function") return "jsqr";
  try {
    const formats = await Detector.getSupportedFormats();
    return formats.includes("qr_code") ? "native" : "jsqr";
  } catch {
    return "jsqr";
  }
}

export interface FrameSource {
  readonly width: number;
  readonly height: number;
  draw(): ImageData | null;
}

/** The text of a code in one frame, or null. */
export async function readFrame(
  reader: Reader,
  env: ScannerEnv,
  video: CanvasImageSource,
  frame: FrameSource,
): Promise<string | null> {
  if (reader === "native" && env.BarcodeDetector) {
    const detector = new env.BarcodeDetector({ formats: ["qr_code"] });
    const found = await detector.detect(video);
    return found[0]?.rawValue ?? null;
  }
  const image = frame.draw();
  if (!image) return null;
  const jsqr = await env.loadJsqr();
  return (
    jsqr(image.data, image.width, image.height, { inversionAttempts: "dontInvert" })?.data ?? null
  );
}

export class CameraError extends Error {
  constructor() {
    super("camera");
    this.name = "CameraError";
  }
}

export interface Scan {
  stop(): void;
  readonly reader: Reader;
}

/** Starts the camera into `video` and calls `onText` for each new code until stopped. */
export async function startScan(
  video: HTMLVideoElement,
  onText: (text: string) => void,
  env: ScannerEnv = defaultScannerEnv(),
  media: MediaDevices | undefined = globalThis.navigator?.mediaDevices,
  intervalMs = 200,
): Promise<Scan> {
  if (!media?.getUserMedia) throw new CameraError();
  let stream: MediaStream;
  try {
    stream = await media.getUserMedia({
      audio: false,
      video: { facingMode: { ideal: "environment" } },
    });
  } catch {
    throw new CameraError();
  }
  const reader = await pickReader(env);
  video.srcObject = stream;
  video.muted = true;
  video.setAttribute("playsinline", "true");
  await video.play().catch(() => undefined);

  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { willReadFrequently: true });
  const frame: FrameSource = {
    get width() {
      return canvas.width;
    },
    get height() {
      return canvas.height;
    },
    draw() {
      if (!context || video.videoWidth === 0) return null;
      const scale = Math.min(1, 640 / video.videoWidth);
      canvas.width = Math.round(video.videoWidth * scale);
      canvas.height = Math.round(video.videoHeight * scale);
      context.drawImage(video, 0, 0, canvas.width, canvas.height);
      return context.getImageData(0, 0, canvas.width, canvas.height);
    },
  };

  let stopped = false;
  let last = "";
  const tick = async () => {
    if (stopped) return;
    try {
      const text = await readFrame(reader, env, video, frame);
      if (text && text !== last && !stopped) {
        last = text;
        onText(text);
      }
    } catch {
      // A frame that cannot be read is skipped.
    }
    if (!stopped) timer = setTimeout(tick, intervalMs);
  };
  let timer = setTimeout(tick, intervalMs);
  return {
    reader,
    stop() {
      stopped = true;
      clearTimeout(timer);
      for (const track of stream.getTracks()) track.stop();
      video.srcObject = null;
    },
  };
}
