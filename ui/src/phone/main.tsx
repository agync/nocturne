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
  const services = await loadServices();
  const root = document.getElementById("root");
  if (services.platform.native) {
    const [{ NativeRoot }, { startInsets }] = await Promise.all([
      import("./native/NativeRoot"),
      import("./native/insets"),
      import("./native/native.css"),
    ]);
    void startInsets();
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
