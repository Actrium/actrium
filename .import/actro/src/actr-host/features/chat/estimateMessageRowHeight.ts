import { IMessage, Progress } from '../../../repository/MessageRepository';

// 中文注释：普通短消息的基础估高，覆盖纯文本问答的主路径。
const BASE_ROW_HEIGHT = 140;
// 中文注释：中等长度文本的估高，减少段落型回答的低估概率。
const MEDIUM_TEXT_ROW_HEIGHT = 190;
// 中文注释：长文本回答的估高，优先避免新消息顶到未完成布局的旧消息。
const LONG_TEXT_ROW_HEIGHT = 250;
// 中文注释：超长文本回答的估高，覆盖多段回答但不包含复杂块内容的情况。
const XL_TEXT_ROW_HEIGHT = 320;
// 中文注释：Markdown 图片块的单次高度加成，按聊天气泡里的常见预览尺寸保守估算。
const MARKDOWN_IMAGE_HEIGHT_BONUS = 180;
// 中文注释：诊断阶段给表格较大的基础高度，用于验证重叠是否主要由表格消息低估触发。
const MARKDOWN_TABLE_BASE_HEIGHT_BONUS = 420;
// 中文注释：诊断阶段放大表格逐行加成，优先排除“表格行数越多越容易重叠”的低估问题。
const MARKDOWN_TABLE_ROW_HEIGHT_BONUS = 72;
// 中文注释：代码块的单次高度加成，避免代码块和表格一样把下一条消息顶上来。
const CODE_BLOCK_HEIGHT_BONUS = 120;
// 中文注释：命中 SvgXml 图表类消息时，Phone 端至少按 300 高度保守估算。
const SVG_XML_MIN_HEIGHT_PHONE = 300;
// 中文注释：命中 SvgXml 图表类消息时，Pad 端至少按 500 高度保守估算。
const SVG_XML_MIN_HEIGHT_TABLET = 500;
// 中文注释：回答仍在流式生成时额外加高，给后续继续长高预留空间。
const STREAMING_ANSWER_HEIGHT_BONUS = 120;
// 中文注释：附件问题气泡的固定高度补偿，覆盖顶部附件条目的占位。
const ATTACHMENT_HEIGHT_BONUS = 96;
// 中文注释：诊断阶段同步抬高估高上限，避免超大表格补偿被上限截断后失去验证意义。
const MAX_ESTIMATED_ROW_HEIGHT = 1400;

function countMarkdownImages(content: string): number {
  // 中文注释：同时匹配 Markdown 图片语法和 HTML img，覆盖回答里两种常见图片来源。
  const markdownMatches = content.match(/!\[[^\]]*]\([^)]+\)/g) ?? [];
  const htmlMatches = content.match(/<img\b[^>]*>/gi) ?? [];
  return markdownMatches.length + htmlMatches.length;
}

export function analyzeMarkdownTables(content: string): {
  tableCount: number;
  totalRowCount: number;
} {
  // 中文注释：统计 Markdown 表格块数量和数据行数，避免多个表格或超长表格仍然只算一次固定补偿。
  const lines = content.split('\n').map((line) => line.trim());
  let tableCount = 0;
  let totalRowCount = 0;

  for (let index = 0; index < lines.length - 1; index += 1) {
    const currentLine = lines[index];
    const nextLine = lines[index + 1];
    if (!currentLine.includes('|')) {
      continue;
    }
    if (/^\|?[\s:-]+(\|[\s:-]+)+\|?$/.test(nextLine)) {
      tableCount += 1;

      let dataRowCount = 0;
      let rowIndex = index + 2;
      // 中文注释：连续的竖线行视为同一个表格的数据区，直到遇到空行或非表格行。
      while (rowIndex < lines.length && lines[rowIndex].includes('|')) {
        dataRowCount += 1;
        rowIndex += 1;
      }

      totalRowCount += dataRowCount;
      index = rowIndex - 1;
    }
  }

  return { tableCount, totalRowCount };
}

function countCodeBlocks(content: string): number {
  // 中文注释：按 fenced code block 计数，覆盖回答里最容易显著拉高高度的代码片段。
  const matches = content.match(/```/g);
  return matches ? Math.max(1, Math.floor(matches.length / 2)) : 0;
}

function countSvgXmlLikeBlocks(content: string): number {
  // 中文注释：diagramNode.tsx 最终落到 SvgXml 的消息通常来自 fenced diagram block，
  // 这里按常见图表/流程图语言名做轻量识别，避免为估高引入渲染层依赖。
  const matches = content.match(
    /```(?:mermaid|plantuml|dot|graphviz|vega|vega-lite|echarts|svg)\b[\s\S]*?```/gi,
  );
  return matches?.length ?? 0;
}

function estimateTextHeight(content: string): number {
  // 中文注释：按字符量和换行数做粗粒度分档，不追求精确，只避免明显低估。
  const normalizedContent = content.trim();
  const lineCount = normalizedContent === '' ? 0 : normalizedContent.split('\n').length;
  const textLength = normalizedContent.length + lineCount * 12;

  if (textLength >= 900) {
    return XL_TEXT_ROW_HEIGHT;
  }
  if (textLength >= 360) {
    return LONG_TEXT_ROW_HEIGHT;
  }
  if (textLength >= 120) {
    return MEDIUM_TEXT_ROW_HEIGHT;
  }
  return BASE_ROW_HEIGHT;
}

export function estimateMessageRowHeight(
  message: IMessage,
  isLatest: boolean,
  isTablet: boolean,
): number {
  // 中文注释：先用问题和回答文本做基础估高，因为一条 row 同时承载整组问答气泡。
  let estimatedHeight = estimateTextHeight(message.question_content || '');
  estimatedHeight += estimateTextHeight(message.answer_content || '') - BASE_ROW_HEIGHT;

  if (message.attachments) {
    // 中文注释：问题附件是独立横向块，直接增加固定高度比重新解析附件内容更稳。
    estimatedHeight += ATTACHMENT_HEIGHT_BONUS;
  }

  if (message.answer_content) {
    const imageCount = countMarkdownImages(message.answer_content);
    const codeBlockCount = countCodeBlocks(message.answer_content);
    const svgXmlLikeBlockCount = countSvgXmlLikeBlocks(message.answer_content);
    const { tableCount, totalRowCount } = analyzeMarkdownTables(
      message.answer_content,
    );

    // 中文注释：回答里存在 Markdown 图片时提高估高，减少图片实际加载后把列表顶乱。
    if (imageCount > 0) {
      estimatedHeight += imageCount * MARKDOWN_IMAGE_HEIGHT_BONUS;
    }

    // 中文注释：回答里有表格时，按表格块数量和数据行数分别补偿，避免复杂表格仍然明显低估。
    if (tableCount > 0) {
      estimatedHeight += tableCount * MARKDOWN_TABLE_BASE_HEIGHT_BONUS;
      estimatedHeight += totalRowCount * MARKDOWN_TABLE_ROW_HEIGHT_BONUS;
    }

    // 中文注释：代码块同样会显著拉高回答气泡，做单独补偿避免和表格场景同类问题。
    if (codeBlockCount > 0) {
      estimatedHeight += codeBlockCount * CODE_BLOCK_HEIGHT_BONUS;
    }

    // 中文注释：命中 SvgXml 图表类消息时，以设备维度给每个图表块设置保底高度，
    // 避免图表尚未测量完成前列表把后续消息排得过高。
    if (svgXmlLikeBlockCount > 0) {
      estimatedHeight += svgXmlLikeBlockCount * (
        isTablet ? SVG_XML_MIN_HEIGHT_TABLET : SVG_XML_MIN_HEIGHT_PHONE
      );
    }
  }

  if (
    isLatest &&
    (message.progress === Progress.pending ||
      message.progress === Progress.processing)
  ) {
    // 中文注释：最新回答处于流式中时预留额外空间，降低边生成边发送新消息时的错位概率。
    estimatedHeight += STREAMING_ANSWER_HEIGHT_BONUS;
  }

  return Math.min(Math.max(estimatedHeight, BASE_ROW_HEIGHT), MAX_ESTIMATED_ROW_HEIGHT);
}
