declare module 'host:service' {
  export type ServiceCallInput = {
    route: string;
    payloadBase64: string;
    timeoutMs?: number;
  };

  export type ServiceCallOutput = {
    payloadBase64: string;
  };

  export type ServiceTellInput = {
    route: string;
    payloadBase64: string;
  };

  export type ServiceEvent = {
    event: string;
    payloadBase64: string;
  };

  export function call(input: ServiceCallInput): Promise<ServiceCallOutput>;
  export function tell(input: ServiceTellInput): Promise<void>;
  export function on(handler: (event: ServiceEvent) => void): () => void;
}
