import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";
import "@xterm/xterm/css/xterm.css";

// 不用 StrictMode：开发模式的双重挂载会把每个 SSH 会话建立两次
createRoot(document.getElementById("root")!).render(<App />);
