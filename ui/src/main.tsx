import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { createEngine } from "./engine";
import { createPlatform } from "./platform";
import "./design/global.css";

async function start() {
  const [engine, platform] = await Promise.all([createEngine(), createPlatform()]);
  const root = document.getElementById("root") as HTMLElement;
  createRoot(root).render(<App engine={engine} platform={platform} />);
}

void start();
