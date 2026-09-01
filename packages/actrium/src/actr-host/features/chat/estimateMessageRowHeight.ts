import { IMessage, Progress } from '../../../repository/MessageRepository';

// The base of the regular SMS is overestimated and covers the main path of the pure text question and answer.
const BASE_ROW_HEIGHT = 140;
// The medium-length text is overestimated, reducing the undervalued probability of paragraph-type responses.
const MEDIUM_TEXT_ROW_HEIGHT = 190;
// The long text answers are overestimated, and the new messages are given priority to avoid ending with the uncompleted layout of the old ones.
const LONG_TEXT_ROW_HEIGHT = 250;
// The over-long text answers are overestimated and cover multiple responses without complex block elements.
const XL_TEXT_ROW_HEIGHT = 320;
// Markdown The single height of the picture block is added, estimated conservatively by the common preview size in the chat bubble.
const MARKDOWN_IMAGE_HEIGHT_BONUS = 180;
// The diagnostic phase gives the table a larger base height to verify whether the overlap was triggered by an underestimation of the information in the form.
const MARKDOWN_TABLE_BASE_HEIGHT_BONUS = 420;
// Add a line by line for the magnification form during the diagnostic phase, with priority exclusion“The more rows the more tables are easily overlapping”The underestimation.
const MARKDOWN_TABLE_ROW_HEIGHT_BONUS = 72;
// The single height of the code block is added to avoid the code block being used to top up the next message like the table.
const CODE_BLOCK_HEIGHT_BONUS = 120;
// Hit. SvgXml The following is a video of the event:Phone End at least press 300 Highly conservative estimates.
const SVG_XML_MIN_HEIGHT_PHONE = 300;
// Hit. SvgXml The following is a video of the event:Pad End at least press 500 Highly conservative estimates.
const SVG_XML_MIN_HEIGHT_TABLET = 500;
// The response is added to the current generation and the follow-up is kept in the long, high space.
const STREAMING_ANSWER_HEIGHT_BONUS = 120;
// The problem of afluorinated bubbles is fixed and highly compensated, with the top-covered annex item in place.
const ATTACHMENT_HEIGHT_BONUS = 96;
// The diagnostic phase simultaneously overstates the ceiling and avoids the loss of validation after the super-large scale compensation has been cut off.
const MAX_ESTIMATED_ROW_HEIGHT = 1400;

function countMarkdownImages(content: string): number {
  // Matching at the same time Markdown Photo Syntax and HTML img, overwhelms two common photo sources in the answers.
  const markdownMatches = content.match(/!\[[^\]]*]\([^)]+\)/g) ?? [];
  const htmlMatches = content.match(/<img\b[^>]*>/gi) ?? [];
  return markdownMatches.length + htmlMatches.length;
}

export function analyzeMarkdownTables(content: string): {
  tableCount: number;
  totalRowCount: number;
} {
  // Statistics Markdown Number of table blocks and number of data lines to avoid multiple or excessive tables still being considered as a fixed compensation only once.
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
      // Continuous vertical line lines are considered to be data areas of the same table until you encounter an empty or non-table line.
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
  // Press fenced code block Counts, overwhelms the most visible high-heighted code segments in the answers.
  const matches = content.match(/```/g);
  return matches ? Math.max(1, Math.floor(matches.length / 2)) : 0;
}

function countSvgXmlLikeBlocks(content: string): number {
  // diagramNode.tsx ♪ Finally fell ♪ SvgXml The news usually comes from fenced diagram block,
  // Here's the usual chart./Flowchart language names are easily identified to avoid introducing a layer dependence for overestimation.
  const matches = content.match(
    /```(?:mermaid|plantuml|dot|graphviz|vega|vega-lite|echarts|svg)\b[\s\S]*?```/gi,
  );
  return matches?.length ?? 0;
}

function estimateTextHeight(content: string): number {
  // Slotting by character and line breaks, without seeking precision, only avoiding a clear underestimation.
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
  // First, the question and answer text is used as the basis for the evaluation, since one article row The project will be launched in the next few days.
  let estimatedHeight = estimateTextHeight(message.question_content || '');
  estimatedHeight += estimateTextHeight(message.answer_content || '') - BASE_ROW_HEIGHT;

  if (message.attachments) {
    // The problem annex is a stand-alone horizontal block, with a direct increase in fixed heights more stable than the re-elaboration of the annex.
    estimatedHeight += ATTACHMENT_HEIGHT_BONUS;
  }

  if (message.answer_content) {
    const imageCount = countMarkdownImages(message.answer_content);
    const codeBlockCount = countCodeBlocks(message.answer_content);
    const svgXmlLikeBlockCount = countSvgXmlLikeBlocks(message.answer_content);
    const { tableCount, totalRowCount } = analyzeMarkdownTables(
      message.answer_content,
    );

    // There's an answer. Markdown to reduce the actual loading of the picture and to mess up the top of the list.
    if (imageCount > 0) {
      estimatedHeight += imageCount * MARKDOWN_IMAGE_HEIGHT_BONUS;
    }

    // When tables are available in the responses, they are compensated by the number of tables and the number of data lines, so that the complex tables are not still clearly underestimated.
    if (tableCount > 0) {
      estimatedHeight += tableCount * MARKDOWN_TABLE_BASE_HEIGHT_BONUS;
      estimatedHeight += totalRowCount * MARKDOWN_TABLE_ROW_HEIGHT_BONUS;
    }

    // The code block will also significantly lift the response to bubbles, and will be used as a separate compensation to avoid and table scenes for similar questions.
    if (codeBlockCount > 0) {
      estimatedHeight += codeBlockCount * CODE_BLOCK_HEIGHT_BONUS;
    }

    // Hit. SvgXml When you have chart type messages, set a bottom height for each chart block by the device dimension.
    // To avoid that the chart does not include a list of follow-up messages that is overcaling the list before it is completed.
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
    // The latest response is in the flow mode, leaving additional space to reduce the probability of error when sending new messages by generating them.
    estimatedHeight += STREAMING_ANSWER_HEIGHT_BONUS;
  }

  return Math.min(Math.max(estimatedHeight, BASE_ROW_HEIGHT), MAX_ESTIMATED_ROW_HEIGHT);
}
