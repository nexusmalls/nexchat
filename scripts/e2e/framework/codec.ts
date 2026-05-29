function isPlainObject(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === 'object' && !Array.isArray(value);
}

/**
 * 归一化标识符，便于跨驼峰/下划线风格匹配字段名。
 */
export function normalizeIdentifier(value: string): string {
  return value.replace(/[^a-zA-Z0-9]/g, '').toLowerCase();
}

/**
 * 将 codec 值转换成 JSON 结构。
 */
export function codecToJson<T = unknown>(value: any): T {
  if (value && typeof value.toJSON === 'function') {
    return value.toJSON() as T;
  }
  return value as T;
}

/**
 * 将 codec 值转换成人类可读结构。
 */
export function codecToHuman<T = unknown>(value: any): T {
  if (value && typeof value.toHuman === 'function') {
    return value.toHuman() as T;
  }
  return value as T;
}

/**
 * 在对象中按候选字段名读取值，兼容驼峰和下划线命名。
 */
export function readObjectField(record: unknown, ...candidates: string[]): unknown {
  if (!isPlainObject(record)) {
    return undefined;
  }

  for (const candidate of candidates) {
    if (candidate in record) {
      return record[candidate];
    }

    const normalized = normalizeIdentifier(candidate);
    const matchedKey = Object.keys(record).find((key) => normalizeIdentifier(key) === normalized);
    if (matchedKey) {
      return record[matchedKey];
    }
  }

  return undefined;
}

/**
 * 尝试把不同类型的值转换成 number。
 */
export function coerceNumber(value: unknown): number | undefined {
  if (value == null) {
    return undefined;
  }
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === 'bigint') {
    return Number(value);
  }
  if (typeof value === 'string') {
    const cleaned = value.replace(/,/g, '').trim();
    if (!cleaned) {
      return undefined;
    }
    const parsed = Number(cleaned);
    return Number.isFinite(parsed) ? parsed : undefined;
  }
  return undefined;
}

/**
 * 将文本型 codec 值解码为 UTF-8 字符串。
 */
export function decodeTextValue(value: unknown): string | undefined {
  if (value == null) {
    return undefined;
  }
  if (typeof value === 'string') {
    if (value.startsWith('0x') && value.length % 2 === 0) {
      try {
        return Buffer.from(value.slice(2), 'hex').toString('utf8');
      } catch {
        return value;
      }
    }
    return value;
  }
  if (Array.isArray(value) && value.every((item) => typeof item === 'number')) {
    return new TextDecoder().decode(Uint8Array.from(value));
  }
  return undefined;
}

/**
 * 将任意值转成便于日志展示的字符串描述。
 */
export function describeValue(value: unknown): string {
  if (typeof value === 'string') {
    return value;
  }
  return JSON.stringify(value);
}
