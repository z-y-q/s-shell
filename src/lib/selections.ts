const selections = new Map<string, string>();

export function setSelection(key: string, text: string): void {
  if (text) {
    selections.set(key, text);
  } else {
    selections.delete(key);
  }
}

export function getSelection(key: string): string {
  return selections.get(key) ?? "";
}

export function dropSelection(key: string): void {
  selections.delete(key);
}
