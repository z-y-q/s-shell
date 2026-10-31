import { create } from "zustand";
import { listHosts, type Host } from "../lib/ipc";

interface HostsState {
  hosts: Host[];
  loaded: boolean;
  refresh: () => Promise<void>;
}

export const useHosts = create<HostsState>((set) => ({
  hosts: [],
  loaded: false,
  refresh: async () => {
    const hosts = await listHosts();
    set({ hosts, loaded: true });
  },
}));
