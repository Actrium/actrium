#!/usr/bin/env node

/**
 * dev-start.js
 *
 * Android 开发启动脚本，一条命令完成：
 * 1. adb reverse tcp:9999 — 让真机/模拟器能访问宿主机的 log server
 * 2. 启动 HTTP log server (端口 9999) — 接收 JS 端的远程日志
 * 3. 启动 Metro bundler
 *
 * 用法: node scripts/dev-start.js
 */

const { spawn, execSync } = require('child_process');
const http = require('http');

const LOG_PORT = 9998;

// ── 1. adb reverse ──────────────────────────────────────────────────────────
try {
  execSync(`adb reverse tcp:${LOG_PORT} tcp:${LOG_PORT}`, { stdio: 'pipe' });
  console.log(`\x1b[32m[adb] reverse tcp:${LOG_PORT} -> tcp:${LOG_PORT} OK\x1b[0m`);
} catch {
  console.warn(`\x1b[33m[adb] reverse 失败（设备未连接或 adb 不可用，模拟器可忽略）\x1b[0m`);
}

// ── 2. Log Server ───────────────────────────────────────────────────────────
const COLORS = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  cyan: '\x1b[36m',
};

const server = http.createServer((req, res) => {
  const cors = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
  };

  if (req.method === 'OPTIONS') {
    res.writeHead(200, cors);
    res.end('ok');
    return;
  }

  if (req.method === 'POST' && req.url === '/log') {
    let body = '';
    req.on('data', (chunk) => { body += chunk; });
    req.on('end', () => {
      try {
        const data = JSON.parse(body);
        const level = data.level || 'log';
        const msg = data.msg || '';
        const time = new Date().toLocaleTimeString();

        let color = COLORS.green;
        let prefix = '\uD83D\uDCF1';
        if (level === 'error') { color = COLORS.red; prefix = '\u274C'; }
        else if (level === 'warn') { color = COLORS.yellow; prefix = '\u26A0\uFE0F '; }
        else if (level === 'info') { color = COLORS.cyan; prefix = '\u2139\uFE0F '; }

        const display = msg.length > 200 && level !== 'error'
          ? msg.slice(0, 1400)
          : msg;
        console.log(`${color}[${time}] ${prefix} ${display}${COLORS.reset}`);
      } catch {
        console.log(`[RAW] ${body}`);
      }
      res.writeHead(200, cors);
      res.end('ok');
    });
    return;
  }

  if (req.method === 'GET' && req.url === '/') {
    res.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8', ...cors });
    res.end('Log server running. POST to /log with { level, msg }');
    return;
  }

  res.writeHead(404, cors);
  res.end('Not found');
});

server.listen(LOG_PORT, () => {
  console.log(`${COLORS.cyan}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${COLORS.reset}`);
  console.log(`${COLORS.green}  \uD83D\uDCE1 Log server \u542F\u52A8\u6210\u529F${COLORS.reset}`);
  console.log(`${COLORS.cyan}  \u76D1\u542C\u7AEF\u53E3: ${LOG_PORT}${COLORS.reset}`);
  console.log(`${COLORS.yellow}  JS \u65E5\u5FD7\u5C06\u5728\u4E0B\u65B9\u5B9E\u65F6\u663E\u793A...${COLORS.reset}`);
  console.log(`${COLORS.cyan}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${COLORS.reset}\n`);
});

// ── 3. Metro ────────────────────────────────────────────────────────────────
const metro = spawn('npx', ['react-native', 'start', '--reset-cache'], {
  cwd: __dirname + '/..',
  stdio: 'inherit',
  shell: true,
});

metro.on('close', (code) => {
  console.log(`Metro exited (${code})`);
  server.close();
  process.exit(code || 0);
});

// Ctrl+C 清理
process.on('SIGINT', () => {
  metro.kill('SIGINT');
  server.close();
  process.exit(0);
});
process.on('SIGTERM', () => {
  metro.kill('SIGTERM');
  server.close();
  process.exit(0);
});
