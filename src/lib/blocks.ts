// 命令块注册表：TerminalView 写入，BlocksDrawer 读取与跳转
export interface CommandBlock {
  cmd: string;
  start: number;
  end: number;
}

interface Entry {
  blocks: CommandBlock[];
  scroll?: (line: number) => void;
  copyAt?: (block: CommandBlock) => string;
}

const registry = new Map<string, Entry>();

export function updateBlocks(key: string, blocks: CommandBlock[]): void {
  const entry = registry.get(key) ?? { blocks };
  entry.blocks = blocks;
  registry.set(key, entry);
  window.dispatchEvent(new CustomEvent("sshell-blocks", { detail: { key } }));
}

export function registerTerm(
  key: string,
  scroll: (line: number) => void,
  copyAt: (block: CommandBlock) => string,
): void {
  const entry = registry.get(key) ?? { blocks: [] };
  entry.scroll = scroll;
  entry.copyAt = copyAt;
  registry.set(key, entry);
}

export function getEntry(key: string): Entry | undefined {
  return registry.get(key);
}

export function dropBlocks(key: string): void {
  registry.delete(key);
}
