declare module "zmodem.js" {
  // zmodem.js 无官方类型，按源码实际 API 声明：
  // Sentry 构造器要求 to_terminal / on_detect / on_retract / sender 四个键
  interface ZmodemSentryHandlers {
    to_terminal: (octets: Uint8Array) => void;
    sender: (octets: Uint8Array) => void;
    on_detect: (detection: ZmodemDetection) => void;
    on_retract: () => void;
  }

  interface ZmodemDetection {
    confirm: () => ZmodemSession;
  }

  interface ZmodemSession {
    get_session_role(): "receive" | "send";
    start(): Promise<unknown>;
    close(): Promise<unknown>;
    /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
    on(event: string, handler: (...args: any[]) => void): void;
    send_offer(fileParams: unknown): Promise<unknown>;
  }

  interface ZmodemBrowser {
    send_files(zsession: ZmodemSession, files: File[], opts?: unknown): Promise<void>;
    save_to_disk(parts: Uint8Array[], filename: string): void;
  }

  const Zmodem: {
    Sentry: new (handlers: ZmodemSentryHandlers) => {
      consume: (octets: Uint8Array) => void;
      send_files(zsession: ZmodemSession, files: File[]): Promise<void>;
    };
    Browser: ZmodemBrowser;
  };

  export default Zmodem;
}
