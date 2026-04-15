import type { SupramarkConfig } from '@supramark/core';
import { createCoreMarkdownFeatureConfig } from '@supramark/feature-core-markdown';
import { createGfmFeatureConfig } from '@supramark/feature-gfm';
import { createMathFeatureConfig } from '@supramark/feature-math';
import { createDefinitionListFeatureConfig } from '@supramark/feature-definition-list';
import { createDiagramDotFeatureConfig } from '@supramark/feature-diagram-dot';
import { createDiagramPlantUmlFeatureConfig } from '@supramark/feature-diagram-plantuml';
import { createDiagramVegaLiteFeatureConfig } from '@supramark/feature-diagram-vega-lite';
import { createDiagramEchartsFeatureConfig } from '@supramark/feature-diagram-echarts';
import { createDiagramMermaidFeatureConfig } from '@supramark/feature-diagram-mermaid';
import {
  admonitionFeature,
  renderAdmonitionContainerRN,
} from '@supramark/feature-admonition';
import { renderWeatherContainerRN, weatherFeature } from '@supramark/feature-weather';

const SUPRAMARK_PARSER_REGISTRY_KEY = '__askaway_supramark_parser_registry__';

function registerParserOnce(featureId: string, register?: () => void) {
  if (!register) return;
  const g = globalThis as typeof globalThis & {
    [SUPRAMARK_PARSER_REGISTRY_KEY]?: Set<string>;
  };
  if (!g[SUPRAMARK_PARSER_REGISTRY_KEY]) {
    g[SUPRAMARK_PARSER_REGISTRY_KEY] = new Set<string>();
  }
  const registry = g[SUPRAMARK_PARSER_REGISTRY_KEY]!;
  if (registry.has(featureId)) return;
  register();
  registry.add(featureId);
}

registerParserOnce('@supramark/feature-admonition', admonitionFeature.registerParser);
registerParserOnce('@supramark/feature-weather', weatherFeature.registerParser);

// 统一的 Supramark 全量能力配置（FeatureConfig 形态，供 parseMarkdown 使用）
export const SUPRAMARK_FULL_CONFIG: SupramarkConfig = {
  features: [
    createCoreMarkdownFeatureConfig(true),
    createGfmFeatureConfig(true),
    createMathFeatureConfig(true),
    createDefinitionListFeatureConfig(true),
    createDiagramDotFeatureConfig(true),
    createDiagramPlantUmlFeatureConfig(true),
    createDiagramVegaLiteFeatureConfig(true),
    createDiagramEchartsFeatureConfig(true),
    createDiagramMermaidFeatureConfig(true),
    { id: '@supramark/feature-footnote', enabled: true },
    { id: '@supramark/feature-emoji', enabled: true },
    { id: '@supramark/feature-admonition', enabled: true },
    { id: '@supramark/feature-weather', enabled: true },
  ],
  diagram: {
    defaultTimeoutMs: 10000,
    defaultCache: {
      enabled: true,
      maxSize: 200,
      ttl: 5 * 60 * 1000,
    },
    engines: {
      plantuml: {
        server: 'https://www.plantuml.com/plantuml/svg',
      },
    },
  },
};

// 容器型 feature 的 RN 渲染器注册表（避免依赖 @supramark/rn 的自动生成文件）
export const SUPRAMARK_CONTAINER_RENDERERS = {
  admonition: renderAdmonitionContainerRN,
  weather: renderWeatherContainerRN,
};
