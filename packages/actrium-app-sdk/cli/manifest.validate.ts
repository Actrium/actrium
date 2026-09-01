// Internal manifest validation helpers for actr

export function validateName(name: string): void {
  // Letters, numbers, underscore; must start with letter; 1-100
  if (!/^[A-Za-z][A-Za-z0-9_]{0,99}$/.test(name)) {
    throw new Error(
      `manifest.name invalid (expected /^[A-Za-z][A-Za-z0-9_]{0,99}$/): ${JSON.stringify(name)}`
    );
  }
}

export function validateSemver(version: string): void {
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error(
      `manifest.version invalid (expected x.y.z): ${JSON.stringify(version)}`
    );
  }
}

export function validateDescription(description: string): void {
  const s = description.trim();
  if (s.length < 1) throw new Error('manifest.description must be non-empty');
}

export function validateJsPathMaybe(path: unknown, fieldName: string): void {
  if (path === undefined) return;
  if (typeof path !== 'string') throw new Error(`${fieldName} must be a string`);
  if (!path.endsWith('.js')) throw new Error(`${fieldName} must be a .js file path`);
}
