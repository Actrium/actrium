import type React from 'react';

// askit 相关模块已由 tsconfig.json paths 直接指向真实源码，无需在此声明
// 此文件仅用于 rill 和 @supramark/* 等无本地源码的外部模块隔离

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
  export type SupramarkContentStyle = Record<string, any>;
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
