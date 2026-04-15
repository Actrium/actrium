import type React from 'react';

// 中文注释：CI 只校验 loom 自身时，用最小声明边界隔离跨仓源码和本地 file: 依赖。

declare module 'askit' {
  export const ThemeView: React.ComponentType<any>;
  export const StepList: React.ComponentType<any>;
  export const UserAvatar: React.ComponentType<any>;
  export const ChatBubble: React.ComponentType<any>;
  export const MyTouchableOpacity: React.ComponentType<any>;
  export const DropdownMenu: React.ComponentType<any>;
  export const DropdownMenuItem: React.ComponentType<any>;
  export const DropdownMenuText: React.ComponentType<any>;
  export const DropdownMenuSubText: React.ComponentType<any>;
}

declare module 'askit/core' {
  export function createEngineAdapter(
    engine: any,
    options?: {
      onContractViolation?: (violation: any) => void;
      permissions?: any[];
      permissionMode?: string;
    },
  ): { dispose: () => void };
}

declare module 'askit/contracts' {
  export type ContractViolation = any;

  export type GuestToHostEventPayloads = {
    SPEECH_REQUEST: {
      requestId: string;
      text?: string;
      action?: string;
      type?: string;
    };
    HTTP_REQUEST: {
      requestId: string;
      url: string;
      method?: string;
      headers?: Record<string, string>;
      body?: unknown;
    };
    [key: string]: any;
  };

  export type HostToGuestEventPayloads = {
    SPEECH_RESPONSE: {
      requestId: string;
      success: boolean;
      error?: {
        code: number;
        message: string;
      };
    };
    HTTP_RESPONSE: {
      requestId: string;
      success: boolean;
      status: number;
      message?: string;
      data?: unknown;
    };
    [key: string]: any;
  };

  export function createHostToGuestSender(
    engine: any,
    options?: { onViolation?: (violation: ContractViolation) => void },
  ): (event: string, payload: unknown) => void;

  export function createContractViolationCollector(
    options?: { max?: number },
  ): {
    record: (violation: ContractViolation) => void;
    summary: () => any;
  };
}

declare module 'rill' {
  export type ComponentMap = Record<string, React.ComponentType<any>>;

  export class Engine {
    constructor(options?: any);
    id: string;
    isLoaded: boolean;
    isDestroyed: boolean;
    isPaused: boolean;
    register(components: ComponentMap): void;
    createReceiver(onUpdate?: () => void): any;
    getReceiver(): any;
    loadBundle(bundleUrl: string, options?: any): Promise<void>;
    destroy(): void;
    pause(): void;
    resume(): void;
    sendEvent(event: string, payload: unknown): void;
    on(event: string, listener: (...args: any[]) => any): () => void;
    getDiagnostics?(): any;
  }
}

declare module 'rill/host/preset' {
  export const ActivityIndicator: React.ComponentType<any>;
  export const Button: React.ComponentType<any>;
  export const Image: React.ComponentType<any>;
  export const ScrollView: React.ComponentType<any>;
  export const Text: React.ComponentType<any>;
  export const TextInput: React.ComponentType<any>;
  export const TouchableOpacity: React.ComponentType<any>;
  export const View: React.ComponentType<any>;
  export const FlatList: React.ComponentType<any>;
  export const Switch: React.ComponentType<any>;
}

declare module '@supramark/core' {
  export type SupramarkConfig = {
    features?: Array<any>;
    diagram?: Record<string, any>;
  };
}

declare module '@supramark/rn' {
  export const Supramark: React.ComponentType<any>;
  export type SupramarkStyles = Record<string, any>;
}

declare module '@supramark/feature-core-markdown' {
  export function createCoreMarkdownFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-gfm' {
  export function createGfmFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-math' {
  export function createMathFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-definition-list' {
  export function createDefinitionListFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-diagram-dot' {
  export function createDiagramDotFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-diagram-plantuml' {
  export function createDiagramPlantUmlFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-diagram-vega-lite' {
  export function createDiagramVegaLiteFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-diagram-echarts' {
  export function createDiagramEchartsFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-diagram-mermaid' {
  export function createDiagramMermaidFeatureConfig(enabled?: boolean): any;
}

declare module '@supramark/feature-admonition' {
  export const admonitionFeature: {
    registerParser?: () => void;
  };
  export const renderAdmonitionContainerRN: React.ComponentType<any>;
}

declare module '@supramark/feature-weather' {
  export const weatherFeature: {
    registerParser?: () => void;
  };
  export const renderWeatherContainerRN: React.ComponentType<any>;
}
