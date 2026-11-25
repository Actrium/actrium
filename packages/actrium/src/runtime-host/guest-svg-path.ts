const argumentCounts: Readonly<Record<string, number>> = {
  M: 2, L: 2, H: 1, V: 1, C: 6, S: 4, Q: 4, T: 2, A: 7, Z: 0,
};

export function normalizeGuestSvgPath(data: string): string | undefined {
  const whitespace = /\s*/y;
  const commandToken = /[MmLlHhVvCcSsQqTtAaZz]/y;
  const numberToken = /[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/y;
  const flagToken = /[01]/y;
  const segments: string[] = [];
  let offset = 0;
  let implicitCommand: string | undefined;

  function read(token: RegExp): string | undefined {
    token.lastIndex = offset;
    const match = token.exec(data);
    if (!match) return undefined;
    offset = token.lastIndex;
    return match[0];
  }

  while (offset < data.length) {
    read(whitespace);
    if (offset === data.length) break;
    const explicit = read(commandToken);
    const command = explicit ?? implicitCommand;
    if (!command) return undefined;
    const upper = command.toUpperCase();
    if (segments.length === 0 && upper !== 'M') return undefined;

    const operands: string[] = [];
    for (let index = 0; index < argumentCounts[upper]!; index += 1) {
      read(whitespace);
      if ((!explicit || index > 0) && data[offset] === ',') {
        offset += 1;
        read(whitespace);
      }
      const operand = read(upper === 'A' && (index === 3 || index === 4) ? flagToken : numberToken);
      if (operand === undefined || !Number.isFinite(Math.fround(Number(operand)))) return undefined;
      operands.push(operand);
    }
    // Explicit commands and spaced operands avoid ambiguous tokens in the native parser.
    segments.push(command + operands.join(' '));
    if (upper === 'M') implicitCommand = command === 'M' ? 'L' : 'l';
    else implicitCommand = upper === 'Z' ? undefined : command;
  }
  return segments.length > 0 ? segments.join(' ') : undefined;
}
