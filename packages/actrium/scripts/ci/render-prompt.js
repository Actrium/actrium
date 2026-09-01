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

  // Just take the head end of the log and avoid putting the whole file typecheck Log Injection prompt.
  const logHead = logLines.slice(0, 200).join('\n');
  const logTail = logLines.slice(-200).join('\n');

  prompt = [
    prompt,
    '',
    'First 200 lines of the typecheck log:',
    logHead,
    '',
    'Last 200 lines of the typecheck log:',
    logTail,
  ].join('\n');
}

process.stdout.write(prompt);
