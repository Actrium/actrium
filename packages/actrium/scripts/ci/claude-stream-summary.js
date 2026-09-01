#!/usr/bin/env node

const readline = require("node:readline");

const rl = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});

const toolStates = new Map();
const toolIndexToKey = new Map();
const emittedToolSummaries = new Set();
const emittedFileUpdates = new Set();

function stamp() {
  const now = new Date();
  const hh = String(now.getHours()).padStart(2, "0");
  const mm = String(now.getMinutes()).padStart(2, "0");
  const ss = String(now.getSeconds()).padStart(2, "0");
  return `[${hh}:${mm}:${ss}]`;
}

function emit(message) {
  process.stdout.write(`${stamp()} ${message}\n`);
}

function toolKeyFromContentBlock(block, index) {
  const key = block?.id || `index:${index}`;
  if (index != null) {
    toolIndexToKey.set(index, key);
  }
  return key;
}

function toolKeyFromEvent(event) {
  if (event?.index != null && toolIndexToKey.has(event.index)) {
    return toolIndexToKey.get(event.index);
  }

  if (event?.content_block) {
    return toolKeyFromContentBlock(event.content_block, event.index);
  }

  return `index:${event?.index ?? "unknown"}`;
}

function compactText(value, maxLength = 140) {
  if (typeof value !== "string") return "";
  const normalized = value.replace(/\s+/g, " ").trim();
  if (!normalized) return "";
  return normalized.length > maxLength ? `${normalized.slice(0, maxLength - 1)}…` : normalized;
}

function normalizePath(value) {
  return compactText(String(value || ""));
}

function summarizeToolInput(name, input) {
  if (!input || typeof input !== "object") {
    return name || "unknown";
  }

  // This extracts the summary by input field of a common tool, avoiding the whole paragraph JSON I'm putting it in the log.
  if (name === "Read") {
    const path = normalizePath(input.file_path || input.path);
    return path ? `Read: ${path}` : "Read";
  }

  if (name === "Edit" || name === "Write" || name === "MultiEdit") {
    const path = normalizePath(input.file_path || input.path);
    return path ? `${name}: ${path}` : name;
  }

  if (name === "Glob") {
    const pattern = compactText(input.pattern || input.glob || "");
    const path = normalizePath(input.path || input.base_path || "");
    if (pattern && path) return `Glob: ${pattern} in ${path}`;
    if (pattern) return `Glob: ${pattern}`;
    return "Glob";
  }

  if (name === "Grep") {
    const pattern = compactText(input.pattern || input.query || input.search || "");
    const path = normalizePath(input.path || input.file_path || input.cwd || "");
    if (pattern && path) return `Grep: ${pattern} in ${path}`;
    if (pattern) return `Grep: ${pattern}`;
    return "Grep";
  }

  if (name === "Bash") {
    const command = compactText(
      input.command || input.cmd || input.script || input.shell_command || "",
      220
    );
    return command ? `Bash: ${command}` : "Bash";
  }

  if (name === "TodoWrite") {
    const todos = Array.isArray(input.todos) ? input.todos.length : null;
    return todos == null ? "TodoWrite" : `TodoWrite: ${todos} items`;
  }

  if (name === "Agent" || name === "Task") {
    const description = compactText(
      input.description || input.prompt || input.task || input.message || "",
      180
    );
    return description ? `${name}: ${description}` : name;
  }

  const path = normalizePath(input.file_path || input.path || "");
  if (path) return `${name}: ${path}`;

  return name || "unknown";
}

function ensureToolState(key, name = "unknown") {
  if (!toolStates.has(key)) {
    toolStates.set(key, {
      name,
      partialJson: "",
      input: null,
    });
  }

  const state = toolStates.get(key);
  if (name && state.name === "unknown") {
    state.name = name;
  }
  return state;
}

function emitToolSummary(key) {
  if (emittedToolSummaries.has(key)) return;

  const state = toolStates.get(key);
  if (!state) return;

  let input = state.input;

  // Here's a few more parts. input_json_delta We'll try to parse it when we're together. Read/Edit/Grep specific parameters.
  if (!input && state.partialJson) {
    try {
      input = JSON.parse(state.partialJson);
      state.input = input;
    } catch {
      input = null;
    }
  }

  emit(summarizeToolInput(state.name, input));
  emittedToolSummaries.add(key);
}

rl.on("line", (line) => {
  const text = line.trim();
  if (!text) return;

  let json;
  try {
    json = JSON.parse(text);
  } catch {
    emit(text);
    return;
  }

  if (json.type === "system" && json.subtype === "init") {
    emit(`Init: model=${json.model ?? "unknown"}, permission=${json.permissionMode ?? "unknown"}`);
    return;
  }

  if (json.type === "system" && json.subtype === "api_retry") {
    emit(`API retry ${json.attempt ?? "?"}/${json.max_retries ?? "?"}: ${json.error ?? "unknown"}`);
    return;
  }

  if (json.type === "stream_event" && json.event?.type === "content_block_start") {
    const block = json.event.content_block;
    if (block?.type === "tool_use") {
      const key = toolKeyFromContentBlock(block, json.event.index);
      const state = ensureToolState(key, block.name || "unknown");
      if (block.input && typeof block.input === "object") {
        state.input = block.input;
      }
      if (state.input) {
        emitToolSummary(key);
      }
    }
    return;
  }

  if (
    json.type === "stream_event" &&
    json.event?.type === "content_block_delta" &&
    json.event?.delta?.type === "input_json_delta"
  ) {
    const key = toolKeyFromEvent(json.event);
    const state = ensureToolState(key);
    state.partialJson += json.event.delta.partial_json || "";
    return;
  }

  if (json.type === "stream_event" && json.event?.type === "content_block_stop") {
    const key = toolKeyFromEvent(json.event);
    emitToolSummary(key);
    return;
  }

  if (json.type === "user" && json.tool_use_result?.filePath) {
    const filePath = json.tool_use_result.filePath;
    if (!emittedFileUpdates.has(filePath)) {
      emit(`File updated: ${filePath}`);
      emittedFileUpdates.add(filePath);
    }
    return;
  }

  if (json.type === "result") {
    emit(`Completed: duration_ms=${json.duration_ms ?? "unknown"}, cost_usd=${json.total_cost_usd ?? "unknown"}`);
  }
});
