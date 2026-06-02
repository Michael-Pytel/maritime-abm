import { useEffect, useRef, useState } from "react";
import type { TickMessage } from "../types";

const WS_URL = "ws://localhost:3000/ws";

export function useSimSocket() {
  const [tick, setTick] = useState<TickMessage | null>(null);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    function connect() {
      const ws = new WebSocket(WS_URL);
      wsRef.current = ws;

      ws.onopen = () => setConnected(true);
      ws.onclose = () => {
        setConnected(false);
        // Reconnect after 2 s
        setTimeout(connect, 2000);
      };
      ws.onerror = () => ws.close();
      ws.onmessage = (ev) => {
        try {
          const msg: TickMessage = JSON.parse(ev.data);
          setTick(msg);
        } catch {
          // ignore malformed frames
        }
      };
    }
    connect();
    return () => {
      wsRef.current?.close();
    };
  }, []);

  return { tick, connected };
}
