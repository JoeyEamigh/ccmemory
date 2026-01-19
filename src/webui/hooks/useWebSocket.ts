import { useState, useEffect, useRef, useCallback } from "react";

type Message = { type: string; [key: string]: unknown };

type ConnectionState = "connecting" | "connected" | "disconnected" | "reconnecting";

type UseWebSocketResult = {
  state: ConnectionState;
  connected: boolean;
  messages: Message[];
  send: (message: Message) => void;
  reconnect: () => void;
  reconnectAttempts: number;
};

const INITIAL_RECONNECT_DELAY = 1000;
const MAX_RECONNECT_DELAY = 30000;
const MAX_RECONNECT_ATTEMPTS = 10;

export function useWebSocket(projectId?: string): UseWebSocketResult {
  const [state, setState] = useState<ConnectionState>("disconnected");
  const [messages, setMessages] = useState<Message[]>([]);
  const [reconnectAttempts, setReconnectAttempts] = useState(0);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const reconnectDelayRef = useRef(INITIAL_RECONNECT_DELAY);
  const isUnmountedRef = useRef(false);

  const clearReconnectTimeout = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
  }, []);

  const scheduleReconnect = useCallback(() => {
    if (isUnmountedRef.current) return;
    if (reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
      setState("disconnected");
      return;
    }

    setState("reconnecting");
    const delay = Math.min(
      reconnectDelayRef.current * Math.pow(1.5, reconnectAttempts),
      MAX_RECONNECT_DELAY
    );

    reconnectTimeoutRef.current = setTimeout(() => {
      if (!isUnmountedRef.current) {
        setReconnectAttempts((prev) => prev + 1);
        connect();
      }
    }, delay);
  }, [reconnectAttempts]);

  const connect = useCallback(() => {
    if (typeof window === "undefined") return;
    if (isUnmountedRef.current) return;

    clearReconnectTimeout();

    if (wsRef.current) {
      wsRef.current.close();
      wsRef.current = null;
    }

    setState("connecting");

    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//${window.location.host}/ws${projectId ? `?project=${projectId}` : ""}`;

    try {
      const ws = new WebSocket(wsUrl);
      wsRef.current = ws;

      ws.onopen = () => {
        if (isUnmountedRef.current) {
          ws.close();
          return;
        }
        setState("connected");
        setReconnectAttempts(0);
        reconnectDelayRef.current = INITIAL_RECONNECT_DELAY;
      };

      ws.onmessage = (event) => {
        if (isUnmountedRef.current) return;
        try {
          const data = JSON.parse(event.data as string) as Message;
          setMessages((prev) => [...prev, data]);
        } catch {
          // Ignore malformed messages
        }
      };

      ws.onclose = (event) => {
        if (isUnmountedRef.current) return;

        wsRef.current = null;

        if (event.code === 1000 || event.code === 1001) {
          setState("disconnected");
        } else {
          scheduleReconnect();
        }
      };

      ws.onerror = () => {
        // Connection will close and trigger reconnect via onclose
      };
    } catch {
      if (!isUnmountedRef.current) {
        scheduleReconnect();
      }
    }
  }, [projectId, clearReconnectTimeout, scheduleReconnect]);

  const reconnect = useCallback(() => {
    setReconnectAttempts(0);
    reconnectDelayRef.current = INITIAL_RECONNECT_DELAY;
    connect();
  }, [connect]);

  useEffect(() => {
    isUnmountedRef.current = false;
    connect();

    return () => {
      isUnmountedRef.current = true;
      clearReconnectTimeout();
      if (wsRef.current) {
        wsRef.current.close(1000, "Component unmounted");
        wsRef.current = null;
      }
    };
  }, [connect, clearReconnectTimeout]);

  const send = useCallback((message: Message) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message));
      return true;
    }
    return false;
  }, []);

  useEffect(() => {
    if (messages.length > 100) {
      setMessages((prev) => prev.slice(-50));
    }
  }, [messages.length]);

  return {
    state,
    connected: state === "connected",
    messages,
    send,
    reconnect,
    reconnectAttempts,
  };
}
