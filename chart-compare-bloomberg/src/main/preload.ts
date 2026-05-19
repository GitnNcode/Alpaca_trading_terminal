import { contextBridge, ipcRenderer } from "electron";

contextBridge.exposeInMainWorld("ccb", {
  sidecarPort: (): Promise<number> => ipcRenderer.invoke("ccb:sidecar-port"),
  platform: process.platform,
});
