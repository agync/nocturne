import { createRoot } from "react-dom/client";
import "../design/global.css";
import { App } from "./App";
import "./phone.css";
import { loadServices } from "./services";
import { registerWorker } from "./shell";

/** The phone app, served by the computer to the person's paired phones. */
async function start() {
  const services = await loadServices();
  if (!services.scripted && import.meta.env.PROD) registerWorker();
  const root = document.getElementById("root");
  if (root) createRoot(root).render(<App services={services} />);
}

void start();
