import { app, BrowserWindow, ipcMain, shell } from "electron";
import * as path from "path";
import { startSidecar, stopSidecar, sidecarPort } from "./sidecar";

const isDev = !app.isPackaged;

async function createWindow(): Promise<void> {
  const win = new BrowserWindow({
    width: 1600,
    height: 1000,
    minWidth: 1280,
    minHeight: 800,
    backgroundColor: "#000000",
    title: "ALPACA TERMINAL",
    autoHideMenuBar: true,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  });

  win.once("ready-to-show", () => win.show());

  win.webContents.setWindowOpenHandler(({ url }) => {
    void shell.openExternal(url);
    return { action: "deny" };
  });

  if (isDev) {
    await win.loadURL("http://localhost:5173/");
    // win.webContents.openDevTools({ mode: "detach" });
  } else {
    await win.loadFile(path.join(__dirname, "..", "renderer", "index.html"));
  }
}

ipcMain.handle("ccb:sidecar-port", () => sidecarPort());

app.whenReady().then(async () => {
  try {
    await startSidecar();
  } catch (err) {
    console.error("[ccb] sidecar failed to start:", err);
  }
  await createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) void createWindow();
  });
});

app.on("window-all-closed", () => {
  stopSidecar();
  if (process.platform !== "darwin") app.quit();
});

app.on("before-quit", () => stopSidecar());
