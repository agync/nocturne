import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { createEngine } from "./engine";
import { createPlatform } from "./platform";
import "./design/global.css";

// Where the app made the quick capture window see-through, its page is clear from the first
// paint, before the platform or the engine is ready.
if (new URLSearchParams(window.location.search).has("see-through")) {
  document.documentElement.dataset.surface = "floating";
}

async function start() {
  const platform = await createPlatform();
  const engine = await createEngine(platform);
  const root = document.getElementById("root") as HTMLElement;
  createRoot(root).render(<App engine={engine} platform={platform} />);
}

void start();
