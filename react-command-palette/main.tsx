import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

// `App` is the Tauri front-end (wired to the Rust IPC, with a browser-dev mock
// fallback). The overlay `Demo` remains in the repo as a standalone reference.
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
