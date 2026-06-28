const fs = require('fs');
const path = require('path');

const templatePath = process.argv[2];

if (!templatePath) {
  console.error('Usage: node scripts/ci/render-prompt.js <template-path>');
  process.exit(1);
}

const absoluteTemplatePath = path.resolve(process.cwd(), templatePath);
let prompt = fs.readFileSync(absoluteTemplatePath, 'utf8');

prompt = prompt.replace(/\{\{([A-Z0-9_]+)\}\}/g, (_, key) => process.env[key] || '');

if (process.env.TYPECHECK_LOG_PATH) {
  const absoluteLogPath = path.resolve(process.cwd(), process.env.TYPECHECK_LOG_PATH);
  const logLines = fs.readFileSync(absoluteLogPath, 'utf8').split(/\r?\n/);

  // 仅截取日志头尾，避免把整份 typecheck 日志注入 prompt。
  const logHead = logLines.slice(0, 200).join('\n');
  const logTail = logLines.slice(-200).join('\n');

  prompt = [
    prompt,
    '',
    '以下是 typecheck 日志前 200 行：',
    logHead,
    '',
    '以下是 typecheck 日志后 200 行：',
    logTail,
  ].join('\n');
}

process.stdout.write(prompt);
