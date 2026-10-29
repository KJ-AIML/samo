import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./styles/themes.css";

const rootEl = document.getElementById("root");
if (rootEl) {
  ReactDOM.createRoot(rootEl).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}
