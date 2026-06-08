import type { ServerResponse } from "node:http";

export type GatewayEventPath = Array<string | number>;

export interface GatewayChangeEvent {
  id: number;
  operation: string;
  path?: GatewayEventPath;
  time: string;
}

interface Subscriber {
  id: number;
  response: ServerResponse;
}

const encodeSseEvent = (event: GatewayChangeEvent): string => [
  `id: ${event.id}`,
  "event: sync-web-change",
  `data: ${JSON.stringify(event)}`,
  "",
  "",
].join("\n");

export class GatewayEventBroker {
  private nextEventId = 1;
  private nextSubscriberId = 1;
  private subscribers = new Map<number, Subscriber>();

  subscribe(response: ServerResponse): () => void {
    const id = this.nextSubscriberId++;
    this.subscribers.set(id, { id, response });
    response.write(": connected\n\n");

    return () => {
      this.subscribers.delete(id);
    };
  }

  publish(input: { operation: string; path?: GatewayEventPath }): GatewayChangeEvent {
    const event: GatewayChangeEvent = {
      id: this.nextEventId++,
      operation: input.operation,
      path: input.path,
      time: new Date().toISOString(),
    };
    const encoded = encodeSseEvent(event);

    for (const subscriber of this.subscribers.values()) {
      subscriber.response.write(encoded);
    }

    return event;
  }

  keepalive(): void {
    for (const subscriber of this.subscribers.values()) {
      subscriber.response.write(": keepalive\n\n");
    }
  }

  getSubscriberCount(): number {
    return this.subscribers.size;
  }
}

export const isGatewayEventPath = (value: unknown): value is GatewayEventPath =>
  Array.isArray(value) && value.every((segment) =>
    typeof segment === "string" || typeof segment === "number"
  );
