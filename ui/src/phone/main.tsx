import { createRoot } from "react-dom/client";
import "../design/global.css";
import { App } from "./App";
import "./phone.css";
import { loadServices } from "./services";
import { registerWorker } from "./shell";

/**
 * The phone app: served by the computer to the person's paired phones, or inside the phone app,
 * where its files come with the app and there is no service worker.
 */
async function start() {
  // The browser tests' build of the app: its native side is answered in the page.
  const services =
    import.meta.env.VITE_NATIVE_MOCK === "1"
      ? await (await import("./testing/nativeMock")).nativeMockServices()
      : await loadServices();
  const root = document.getElementById("root");
  if (import.meta.env.VITE_PHONE_NATIVE !== "0" && services.platform.native) {
    const [{ NativeRoot }, { startInsets }, { startClock }] = await Promise.all([
      import("./native/NativeRoot"),
      import("./native/insets"),
      import("./native/clock"),
      import("./native/native.css"),
    ]);
    void startInsets();
    await startClock();
    if (root)
      createRoot(root).render(
        <NativeRoot services={services as import("./native/services").NativeServices} />,
      );
    return;
  }
  if (!services.scripted && import.meta.env.PROD) registerWorker();
  if (root) createRoot(root).render(<App services={services} />);
}

void start();
